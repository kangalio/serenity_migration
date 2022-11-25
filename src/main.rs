#![feature(rustc_private)]
#![allow(unused)]

extern crate rustc_driver;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_lint;
extern crate rustc_middle;
extern crate rustc_span;

mod lint;
mod parse;
mod replace;
mod structures;

struct RustcCallbacks;
impl rustc_driver::Callbacks for RustcCallbacks {
    fn config(&mut self, config: &mut rustc_interface::Config) {
        // Called on every crate
        config.register_lints = Some(Box::new(|session, lints| {
            lints.late_passes.push(Box::new(|_cx| Box::new(lint::Lint)));
        }));
    }
}

fn main() {
    let mut args = std::env::args();
    let _current_executable = args.next();

    let mut rustc_args = args.collect::<Vec<_>>();
    // Not sure why we have to manually add sysroot... won't work otherwise
    rustc_args.push("--sysroot".into());
    rustc_args.push(
        "/home/kangalioo/.rustup/toolchains/nightly-2022-11-03-x86_64-unknown-linux-gnu".into(),
    );

    rustc_driver::RunCompiler::new(&rustc_args, &mut RustcCallbacks)
        .run()
        .unwrap();
}
