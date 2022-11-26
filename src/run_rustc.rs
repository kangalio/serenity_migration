//! Provides the Lint struct which implements rustc_lint::LateLintPass

use rustc_lint::LintContext as _;

use crate::{parse::*, replace::*};

static LINT: rustc_lint::Lint = rustc_lint::Lint {
    name: "serenity_0_12_incompatibilities",
    default_level: rustc_lint::Level::Deny,
    desc: "finds code that will not work in serenity 0.12",
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
        "closure-style builders have been replaced in the next version of serenity",
        |b| {
            b.span_note(span, "replace this...").span_suggestion(
                span,
                "...with",
                replacement,
                rustc_errors::Applicability::MachineApplicable,
            )
        },
    );
}

#[derive(Default)]
struct Lint {
    touched_spans: Vec<rustc_span::Span>,
}
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
        if let Some(_overlapped) = self.touched_spans.iter().find(|s| s.overlaps(expr.span)) {
            return;
        }

        if let Some(builder_closure) = parse_builder_closure(cx, expr) {
            emit_replacement(cx, expr.span, &replace_closure(cx, builder_closure));
            self.touched_spans.push(expr.span);
        }
    }

    fn check_stmt(&mut self, cx: &rustc_lint::LateContext<'tcx>, stmt: &rustc_hir::Stmt<'tcx>) {
        if let Some(call_chain) = parse_stmt_as_builder_call_chain(cx, stmt) {
            let replacement = replace_builder_call_chain_stmt(cx, stmt.span.ctxt(), call_chain);
            emit_replacement(cx, stmt.span, &replacement);
        }
    }
}

struct RustcCallbacks;
impl rustc_driver::Callbacks for RustcCallbacks {
    fn config(&mut self, config: &mut rustc_interface::Config) {
        // Called on every crate
        config.register_lints = Some(Box::new(|session, lints| {
            lints.late_passes.push(Box::new(|_cx| Box::new(Lint::default())));
        }));
    }
}

pub fn run_rustc() {
    let mut args = std::env::args();
    let _current_executable = args.next();

    let mut rustc_args = args.collect::<Vec<_>>();
    // Not sure why we have to manually add sysroot... won't work otherwise
    rustc_args.push("--sysroot".into());
    rustc_args.push(
        "/home/kangalioo/.rustup/toolchains/nightly-2022-11-03-x86_64-unknown-linux-gnu".into(),
    );

    rustc_driver::RunCompiler::new(&rustc_args, &mut RustcCallbacks).run().unwrap();
}
