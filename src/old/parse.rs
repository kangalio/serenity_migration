//! Provides functions to parse HIR into our structures

use super::structures::*;

/// `serenity::builder::CreateAMess<'_>` -> `"CreateAMess"
pub fn as_serenity_builder_type(
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

fn parse_call_chain<'hir>(
    cx: &rustc_lint::LateContext<'hir>,
    method: &rustc_hir::PathSegment<'hir>,
    receiver_expr: &rustc_hir::Expr<'hir>,
    args: &'hir [rustc_hir::Expr<'hir>],
) -> Option<BuilderCallChain<'hir>> {
    let mut call_chain = match &receiver_expr.kind {
        // Recurse until we've reached the start of the chain
        rustc_hir::ExprKind::MethodCall(method, receiver_expr, args, _span) => {
            parse_call_chain(cx, method, receiver_expr, args)?
        }
        // We've reached the start of the chain
        rustc_hir::ExprKind::Path(path) => {
            let Some(receiver) = path_as_ident(path) else { return None };
            BuilderCallChain { receiver, calls: Vec::new() }
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
                    BuilderCallArg::Literal(arg)
                }
            })
            .collect::<Vec<_>>(),
    });
    Some(call_chain)
}

pub fn parse_stmt_as_builder_call_chain<'hir>(
    cx: &rustc_lint::LateContext<'hir>,
    stmt: &rustc_hir::Stmt<'hir>,
) -> Option<BuilderCallChain<'hir>> {
    let rustc_hir::StmtKind::Semi(expr) = stmt.kind else { return None };
    let rustc_hir::ExprKind::MethodCall(method, receiver, args, _span) = expr.kind else { return None };
    parse_call_chain(cx, method, receiver, args)
}

fn parse_closure_body<'hir>(
    cx: &rustc_lint::LateContext<'hir>,
    builder_binding: rustc_span::symbol::Ident,
    body: &rustc_hir::Expr<'hir>,
) -> Option<(Vec<&'hir rustc_hir::Stmt<'hir>>, BuilderCallChain<'hir>)> {
    Some(match &body.kind {
        rustc_hir::ExprKind::MethodCall(method, receiver, args, _span) => {
            (Vec::new(), parse_call_chain(cx, method, receiver, args)?)
        }
        rustc_hir::ExprKind::Block(block, _label) => {
            let Some(expr) = block.expr else { return None };
            let rustc_hir::ExprKind::MethodCall(method, receiver, args, _span) = &expr.kind else { return None };
            let mut call_chain = parse_call_chain(cx, method, receiver, args)?;

            let mut stmts = Vec::new();
            for stmt in block.stmts {
                if let Some(stmt_call_chain) = parse_stmt_as_builder_call_chain(cx, stmt) {
                    call_chain.calls.extend(stmt_call_chain.calls);
                } else {
                    stmts.push(stmt);
                }
            }

            (stmts, call_chain)
        }
        rustc_hir::ExprKind::Path(path) => {
            let Some(ident) = path_as_ident(path) else { return None };
            if ident != builder_binding {
                return None;
            }

            (Vec::new(), BuilderCallChain { receiver: builder_binding, calls: Vec::new() })
        }
        _ => return None,
    })
}

pub fn parse_builder_closure<'hir>(
    cx: &rustc_lint::LateContext<'hir>, // Closure body comes from cx, hence 'hir lifetime
    expr: &rustc_hir::Expr<'hir>,
) -> Option<BuilderClosure<'hir>> {
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

    let (stmts, call_chain) = parse_closure_body(cx, builder_binding, closure_body.value)?;

    Some(BuilderClosure {
        builder_type,
        binding: builder_binding.as_str().to_owned(),
        stmts,
        call_chain,
        span: expr.span,
    })
}

// /// In `BuilderType::default()`, returns the `default` span.
// pub fn builder_default_span<'hir>(
//     cx: &rustc_lint::LateContext<'hir>,
//     expr: &rustc_hir::Expr<'hir>,
// ) -> Option<rustc_span::Span> {
//     let syntax_ctxt = expr.span.ctxt();

//     let rustc_hir::ExprKind::Call(fn_, args) = &expr.kind else { return None };
//     let rustc_hir::ExprKind::Path(qpath) = &fn_.kind else { return None };
//     let rustc_hir::QPath::TypeRelative(builder_type, default_method) = qpath else { return None };
//     let Some(_builder_type) = as_serenity_builder_type(cx, builder_type) else { return None };
//     if default_method.ident.as_str() != "default" {
//         return None;
//     };

//     Some(default_method.ident.span)
// }
