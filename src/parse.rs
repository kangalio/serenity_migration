//! Provides functions to parse HIR into our structures

use crate::structures::*;

/// `serenity::builder::CreateAMess<'_>` -> `"CreateAMess"
fn as_serenity_builder_type(
    cx: &rustc_lint::LateContext<'_>,
    ty: &rustc_middle::ty::Ty<'_>,
) -> Option<String> {
    // Peel references
    let mut ty = ty;
    while let rustc_middle::ty::TyKind::Ref(_, inner_ty, _) = ty.kind() {
        ty = inner_ty;
    }

    // Get path of type
    let rustc_middle::ty::TyKind::Adt(adt, _) = ty.kind() else { return None };
    let def_path = cx.tcx.def_path(adt.0 .0.did);

    // Check that we're in serenity::builder::
    if cx.tcx.crate_name(def_path.krate).as_str() != "serenity" {
        return None;
    }
    let [module, .., ty] = &*def_path.data else { return None };
    let rustc_hir::definitions::DefPathData::TypeNs(module) = module.data else { return None };
    let rustc_hir::definitions::DefPathData::TypeNs(ty) = ty.data else { return None };
    if module.as_str() != "builder" {
        return None;
    }

    // Return stripped type name
    Some(ty.as_str().to_owned())
}

fn path_as_ident(qpath: &rustc_hir::QPath<'_>) -> Option<rustc_span::symbol::Ident> {
    let rustc_hir::QPath::Resolved(None, path) = qpath else { return None };
    let [segment] = &path.segments else { return None };
    Some(segment.ident)
}

fn parse_call_chain(
    cx: &rustc_lint::LateContext<'_>,
    method: &rustc_hir::PathSegment<'_>,
    receiver_expr: &rustc_hir::Expr<'_>,
    args: &[rustc_hir::Expr<'_>],
) -> Option<BuilderCallChain> {
    let mut call_chain = match &receiver_expr.kind {
        // Recurse until we've reached the start of the chain
        rustc_hir::ExprKind::MethodCall(method, receiver_expr, args, _span) => {
            parse_call_chain(cx, method, receiver_expr, args)?
        }
        // We've reached the start of the chain
        rustc_hir::ExprKind::Path(path) => {
            let Some(receiver) = path_as_ident(path) else { return None };
            BuilderCallChain {
                receiver,
                calls: Vec::new(),
            }
        }
        _ => return None,
    };

    call_chain.calls.push(BuilderCall {
        field: method.ident,
        args: args
            .iter()
            .map(|arg| {
                if let Some(builder_closure) = parse_builder_closure(cx, arg) {
                    BuilderCallArg::NestedClosure(builder_closure)
                } else {
                    BuilderCallArg::Literal(arg.span)
                }
            })
            .collect::<Vec<_>>(),
    });
    Some(call_chain)
}

pub fn parse_stmt_as_builder_call_chain(
    cx: &rustc_lint::LateContext<'_>,
    stmt: &rustc_hir::Stmt<'_>,
) -> Option<BuilderCallChain> {
    let rustc_hir::StmtKind::Semi(expr) = stmt.kind else { return None };
    let rustc_hir::ExprKind::MethodCall(method, receiver, args, _span) = expr.kind else { return None };
    parse_call_chain(cx, method, receiver, args)
}

fn parse_stmt(
    cx: &rustc_lint::LateContext<'_>,
    stmt: &rustc_hir::Stmt<'_>,
    expected_receiver: rustc_span::symbol::Ident,
) -> PreBuilderCallStatement {
    if let Some(call_chain) = parse_stmt_as_builder_call_chain(cx, stmt) {
        // In `|a| { b.call() }`, a and b must be the same
        if call_chain.receiver == expected_receiver {
            return PreBuilderCallStatement::BuilderCallChain(call_chain);
        }
    }
    PreBuilderCallStatement::Verbatim(stmt.span)
}

fn parse_closure_body(
    cx: &rustc_lint::LateContext<'_>,
    builder_binding: rustc_span::symbol::Ident,
    body: &rustc_hir::Body<'_>,
) -> Option<(Vec<PreBuilderCallStatement>, BuilderCallChain)> {
    Some(match &body.value.kind {
        rustc_hir::ExprKind::MethodCall(method, receiver, args, _span) => {
            (Vec::new(), parse_call_chain(cx, method, receiver, args)?)
        }
        rustc_hir::ExprKind::Block(block, _label) => {
            let stmts = block
                .stmts
                .iter()
                .map(|stmt| parse_stmt(cx, stmt, builder_binding))
                .collect::<Vec<_>>();

            let Some(expr) = block.expr else { return None };
            let rustc_hir::ExprKind::MethodCall(method, receiver, args, _span) = expr.kind else { return None };
            let call_chain = parse_call_chain(cx, method, receiver, args)?;

            (stmts, call_chain)
        }
        rustc_hir::ExprKind::Path(path) => {
            let Some(ident) = path_as_ident(path) else { return None };
            if ident != builder_binding {
                return None;
            }

            (Vec::new(), BuilderCallChain {
                receiver: builder_binding,
                calls: Vec::new(),
            })
        }
        _ => return None,
    })
}

pub fn parse_builder_closure(
    cx: &rustc_lint::LateContext<'_>,
    expr: &rustc_hir::Expr<'_>,
) -> Option<BuilderClosure> {
    let rustc_hir::ExprKind::Closure(closure) = &expr.kind else { return None };
    let closure_body = cx.tcx.hir().body(closure.body);

    let builder_type = {
        let [param_ty] = closure.fn_decl.inputs else { return None };
        let param_ty = cx.typeck_results().node_type(param_ty.hir_id);
        let rustc_middle::ty::TyKind::Ref(_, builder, rustc_middle::mir::Mutability::Mut) = param_ty.kind() else { return None };
        as_serenity_builder_type(cx, builder)?
    };

    let builder_binding = {
        let [param] = closure_body.params else { return None };
        let rustc_hir::PatKind::Binding(_, _, binding, _) = param.pat.kind else { return None };
        binding
    };

    let (stmts, call_chain) = parse_closure_body(cx, builder_binding, closure_body)?;

    Some(BuilderClosure {
        builder_type,
        binding: builder_binding.as_str().to_owned(),
        stmts,
        call_chain,
        span: expr.span,
    })
}
