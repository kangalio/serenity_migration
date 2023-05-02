//! Runs rustc and passes our lint implementation into it

#![feature(rustc_private)]
#![allow(unused)]

extern crate rustc_driver;
extern crate rustc_errors;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_lint;
extern crate rustc_middle;
extern crate rustc_resolve;
extern crate rustc_span;

mod parse;
mod replace;
mod run_rustc;
mod structures;

fn main() {
    run_rustc::run_rustc();
}
