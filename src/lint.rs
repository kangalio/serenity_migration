use rustc_lint::LintContext as _;

use crate::{parse::*, replace::*};

static LINT: rustc_lint::Lint = rustc_lint::Lint {
    name: "old_builders",
    default_level: rustc_lint::Level::Deny,
    desc: "finds instances of 0.11-style builder closures",
    edition_lint_opts: None,
    report_in_external_macro: false,
    future_incompatible: None,
    is_plugin: true,
    feature_gate: None,
    crate_level_only: true,
};

fn emit_replacement(cx: &rustc_lint::LateContext<'_>, span: rustc_span::Span, replacement: &str) {
    cx.lint(
        &LINT,
        "closure-style builders will break in the next version of serenity",
        |b| {
            b.span_note(
                span,
                "closure-style builders will break in the next version of serenity",
            )
            .span_suggestion(
                span,
                "replace with",
                replacement,
                rustc_errors::Applicability::MachineApplicable,
            )
        },
    );
}

pub struct Lint;
impl rustc_lint::LintPass for Lint {
    fn name(&self) -> &'static str {
        LINT.name
    }
}
impl<'tcx> rustc_lint::LateLintPass<'tcx> for Lint {
    fn check_expr(
        &mut self,
        cx: &rustc_lint::LateContext<'tcx>,
        expr: &'tcx rustc_hir::Expr<'tcx>,
    ) {
        if let Some(builder_closure) = parse_builder_closure(cx, expr) {
            emit_replacement(cx, expr.span, &replace_closure(cx, builder_closure));
        }
    }

    fn check_stmt(&mut self, cx: &rustc_lint::LateContext<'tcx>, stmt: &rustc_hir::Stmt<'tcx>) {
        if let Some(call_chain) = stmt_to_builder_call_chain(cx, stmt) {
            let replacement = replace_builder_call_chain_stmt(cx, stmt.span.ctxt(), call_chain);
            emit_replacement(cx, stmt.span, &replacement);
        }
    }
}
