//! Provides structures used by the rest of the program that represent instances of the builder
//! pattern

#[derive(Debug)]
pub enum PreBuilderCallStatement {
    Verbatim(rustc_span::Span),
    /// Like the `b.foo().bar();` in `.builder(|b| { b.foo().bar(); b.field() })`
    BuilderCallChain(BuilderCallChain),
}

#[derive(Debug)]
pub enum BuilderCallArg {
    /// Like in `.content("hi!")`
    Literal(rustc_span::Span),
    /// Like in `.embed(|e| e.title("my embed"))`
    NestedClosure(BuilderClosure),
}

/// Example: `.method_name(arg1, arg2)`
#[derive(Debug)]
pub struct BuilderCall {
    pub field: rustc_span::symbol::Ident,
    pub args: Vec<BuilderCallArg>,
}

#[derive(Debug)]
pub struct BuilderCallChain {
    pub receiver: rustc_span::symbol::Ident,
    pub receiver_type: String,
    pub calls: Vec<BuilderCall>,
}

#[derive(Debug)]
pub struct BuilderClosure {
    /// Type of the passed in builder, e.g. `CreateEmbed`
    pub builder_type: String,
    pub binding: String,
    /// Like `stmts` in `.send_message(|b| { stmts; b.content() })`
    pub stmts: Vec<PreBuilderCallStatement>,
    pub call_chain: BuilderCallChain,
    pub span: rustc_span::Span,
}
