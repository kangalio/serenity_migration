fn dbg<T: std::fmt::Debug>(x: &T) {
    let mut s = format!("{:?}", x);
    println!("{}", &s[..s.len().min(150)]);
}

pub trait Node<'hir> {
    fn cx(&self) -> Context<'hir>;
    fn span(&self) -> rustc_span::Span;

    fn source_code(&self) -> String {
        self.cx()
            .tcx
            .sess
            .source_map()
            .span_to_snippet(rustc_span::hygiene::walk_chain(self.span(), self.span().ctxt()))
            .unwrap_or_else(|_| "todo!()".to_owned())
    }
}

#[derive(Copy, Clone)]
pub struct Context<'hir> {
    tcx: rustc_middle::ty::TyCtxt<'hir>,
    typeck_results: &'hir rustc_middle::ty::TypeckResults<'hir>,
}
impl<'hir> Context<'hir> {
    fn typeck_results(&self) -> &'hir rustc_middle::ty::TypeckResults<'hir> {
        self.typeck_results
    }
}
impl std::fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Context { .. }")
    }
}

pub struct Path<'hir> {
    cx: Context<'hir>,
    inner: rustc_hir::definitions::DefPath,
}
impl<'hir> Path<'hir> {
    pub fn parts(&self) -> Vec<String> {
        let mut parts = vec![self.cx.tcx.crate_name(self.inner.krate).as_str().to_string()];
        parts.extend(self.inner.data.iter().filter_map(|part| {
            let part = match part.data {
                rustc_hir::definitions::DefPathData::TypeNs(part) => part,
                rustc_hir::definitions::DefPathData::ValueNs(part) => part,
                rustc_hir::definitions::DefPathData::MacroNs(part) => part,
                rustc_hir::definitions::DefPathData::LifetimeNs(part) => part,
                _ => return None,
            };
            Some(part.as_str().to_string())
        }));
        parts
    }
}

pub struct Adt<'hir> {
    cx: Context<'hir>,
    inner: &'hir rustc_middle::ty::AdtDef<'hir>,
}
impl<'hir> Adt<'hir> {
    pub fn path(&self) -> Path<'hir> {
        Path { cx: self.cx, inner: self.cx.tcx.def_path(self.inner.0 .0.did) }
    }
}

pub struct Type<'hir> {
    cx: Context<'hir>,
    inner: rustc_middle::ty::Ty<'hir>,
}
impl<'hir> Type<'hir> {
    pub fn ref_(&self) -> Option<Self> {
        match self.inner.kind() {
            rustc_middle::ty::TyKind::Ref(_, inner, _) => {
                Some(Self { cx: self.cx, inner: inner.clone() })
            }
            _ => None,
        }
    }

    pub fn adt(&self) -> Option<Adt<'hir>> {
        let rustc_middle::ty::TyKind::Adt(adt, _) = self.inner.kind() else { return None };
        Some(Adt { cx: self.cx, inner: adt })
    }
}
pub struct Param<'hir> {
    cx: Context<'hir>,
    inner: &'hir rustc_hir::Param<'hir>,
    type_: &'hir rustc_hir::Ty<'hir>,
}
#[rustfmt::skip]
impl<'hir> Node<'hir> for Param<'hir> {
    fn cx(&self) -> Context<'hir> { self.cx }
    fn span(&self) -> rustc_span::Span { self.inner.span }
}
impl<'hir> Param<'hir> {
    pub fn type_(&self) -> Type<'hir> {
        Type { cx: self.cx, inner: self.cx.typeck_results().node_type(self.type_.hir_id) }
    }
}
#[derive(Debug)]
pub struct Closure<'hir> {
    cx: Context<'hir>,
    inner: &'hir rustc_hir::Closure<'hir>,
}
#[rustfmt::skip]
impl<'hir> Node<'hir> for Closure<'hir> {
    fn cx(&self) -> Context<'hir> { self.cx }
    fn span(&self) -> rustc_span::Span { self.cx.tcx.def_span(self.inner.def_id) }
}
impl<'hir> Closure<'hir> {
    fn hir_body(&self) -> &'hir rustc_hir::Body<'hir> {
        self.cx.tcx.hir().body(self.inner.body)
    }

    pub fn body(&self) -> Expr<'hir> {
        Expr { cx: self.cx, inner: self.hir_body().value }
    }

    pub fn args(&self) -> impl Iterator<Item = Param<'hir>> + 'hir {
        let cx = self.cx;
        Iterator::zip(self.hir_body().params.iter(), self.inner.fn_decl.inputs.iter())
            .map(move |(param, type_)| Param { cx, inner: param, type_ })
    }

    pub fn single_param(&self) -> Option<Param<'hir>> {
        let mut args = self.args();
        args.next().filter(|_| args.next().is_none())
    }
}
#[derive(Debug)]
pub struct MethodCall<'hir> {
    cx: Context<'hir>,
    receiver: &'hir rustc_hir::Expr<'hir>,
    method: &'hir rustc_hir::PathSegment<'hir>,
    args: &'hir [rustc_hir::Expr<'hir>],
    span: rustc_span::Span,
}
#[rustfmt::skip]
impl<'hir> Node<'hir> for MethodCall<'hir> {
    fn cx(&self) -> Context<'hir> { self.cx }
    fn span(&self) -> rustc_span::Span { self.span }
}
impl<'hir> MethodCall<'hir> {
    pub fn receiver(&self) -> Expr<'hir> {
        Expr { cx: self.cx, inner: self.receiver }
    }

    pub fn method_name(&self) -> String {
        self.method.ident.to_string()
    }

    pub fn args(&self) -> impl Iterator<Item = Expr<'hir>> + 'hir {
        let cx = self.cx;
        self.args.iter().map(move |arg| Expr { cx, inner: arg })
    }
}
#[derive(Debug)]
pub struct Expr<'hir> {
    cx: Context<'hir>,
    inner: &'hir rustc_hir::Expr<'hir>,
}
#[rustfmt::skip]
impl<'hir> Node<'hir> for Expr<'hir> {
    fn cx(&self) -> Context<'hir> { self.cx }
    fn span(&self) -> rustc_span::Span { self.inner.span }
}
impl<'hir> Expr<'hir> {
    pub fn new(cx: &rustc_lint::LateContext<'hir>, inner: &'hir rustc_hir::Expr<'hir>) -> Self {
        let cx = Context { tcx: cx.tcx, typeck_results: cx.typeck_results() };
        Self { cx, inner }
    }

    pub fn closure(&self) -> Option<Closure<'hir>> {
        match self.inner.kind {
            rustc_hir::ExprKind::Closure(inner) => Some(Closure { cx: self.cx, inner }),
            _ => None,
        }
    }

    pub fn method_call(&self) -> Option<MethodCall<'hir>> {
        match self.inner.kind {
            rustc_hir::ExprKind::MethodCall(method, receiver, args, span) => {
                Some(MethodCall { cx: self.cx, method, receiver, args, span })
            }
            _ => None,
        }
    }

    pub fn single_expr_block(&self) -> Option<Expr<'hir>> {
        let rustc_hir::ExprKind::Block(block, _) = self.inner.kind else { return None };
        if !block.stmts.is_empty() {
            return None;
        }
        Some(Expr { cx: self.cx, inner: block.expr? })
    }
}
