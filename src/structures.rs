//! Provides structures used by the rest of the program that represent instances of the builder
//! pattern

#[derive(Debug)]
pub enum PreBuilderCallStatement<'hir> {
    Verbatim(rustc_span::Span),
    /// Like the `b.foo().bar();` in `.builder(|b| { b.foo().bar(); b.field() })`
    BuilderCallChain(BuilderCallChain<'hir>),
}

#[derive(Debug)]
pub enum BuilderCallArg<'hir> {
    /// Like in `.content("hi!")`
    Literal(&'hir rustc_hir::Expr<'hir>),
    /// Like in `.embed(|e| e.title("my embed"))`
    NestedClosure(BuilderClosure<'hir>),
}

/// Example: `.method_name(arg1, arg2)`
#[derive(Debug)]
pub struct BuilderCall<'hir> {
    pub field: rustc_span::symbol::Ident,
    pub args: Vec<BuilderCallArg<'hir>>,
}

#[derive(Debug)]
pub struct BuilderCallChain<'hir> {
    pub receiver: rustc_span::symbol::Ident,
    pub calls: Vec<BuilderCall<'hir>>,
}

#[derive(Debug)]
pub struct BuilderClosure<'hir> {
    /// Type of the passed in builder, e.g. `CreateEmbed`
    pub builder_type: String,
    pub binding: String,
    /// Like `stmts` in `.send_message(|b| { stmts; b.content() })`
    pub stmts: Vec<PreBuilderCallStatement<'hir>>,
    pub call_chain: BuilderCallChain<'hir>,
    pub span: rustc_span::Span,
}
