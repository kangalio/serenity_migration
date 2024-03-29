//! Provides the Lint struct which implements rustc_lint::LateLintPass

use rustc_lint::LintContext as _;

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

struct Visitor<'hir, 'anon> {
    cx: &'anon rustc_lint::LateContext<'hir>,
}
impl<'hir> rustc_hir::intravisit::Visitor<'hir> for Visitor<'hir, '_> {
    type NestedFilter = rustc_middle::hir::nested_filter::OnlyBodies;
    type Map = rustc_middle::hir::map::Map<'hir>;

    fn nested_visit_map(&mut self) -> Self::Map {
        self.cx.tcx.hir()
    }

    fn visit_expr(&mut self, expr: &'hir rustc_hir::Expr<'hir>) {
        if let Some(replacement) = crate::migrate::migrate(crate::nodes::Expr::new(self.cx, expr)) {
            emit_replacement(self.cx, expr.span, &replacement);
        } else {
            rustc_hir::intravisit::walk_expr(self, expr);
        }
    }
}

#[derive(Default)]
struct Lint;
impl rustc_lint::LintPass for Lint {
    fn name(&self) -> &'static str {
        LINT.name
    }
}
impl<'tcx> rustc_lint::LateLintPass<'tcx> for Lint {
    fn check_fn(
        &mut self,
        cx: &rustc_lint::LateContext<'tcx>,
        kind: rustc_hir::intravisit::FnKind<'tcx>,
        _: &'tcx rustc_hir::FnDecl<'tcx>,
        body: &'tcx rustc_hir::Body<'tcx>,
        span: rustc_span::Span,
        _: rustc_span::def_id::LocalDefId,
    ) {
        use rustc_hir::intravisit::Visitor as _;
        if let rustc_hir::intravisit::FnKind::Closure = kind {
            return;
        }
        let mut visitor = Visitor { cx };
        visitor.visit_body(body);
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
        "/home/kangalioo/.rustup/toolchains/nightly-2023-05-03-x86_64-unknown-linux-gnu".into(),
    );

    rustc_driver::RunCompiler::new(&rustc_args, &mut RustcCallbacks).run().unwrap();
}
