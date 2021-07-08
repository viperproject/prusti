// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![feature(rustc_private)]
#![feature(proc_macro_internals)]
#![feature(decl_macro)]
#![feature(box_syntax)]

#![deny(unused_must_use)]

extern crate proc_macro;
extern crate rustc_ast;
extern crate rustc_ast_pretty;
extern crate rustc_data_structures;
extern crate rustc_driver;
extern crate rustc_expand;
extern crate rustc_hir;
extern crate rustc_interface;
extern crate rustc_metadata;
extern crate rustc_middle;
extern crate rustc_parse;
extern crate rustc_resolve;
extern crate rustc_session;
extern crate rustc_span;
extern crate rustc_errors;
extern crate smallvec;
extern crate regex;
extern crate prusti_common;

mod callbacks;
mod verifier;
mod arg_value;

use std::{env, panic, borrow::Cow, path::PathBuf};
use prusti_common::report::user;
use lazy_static::lazy_static;
use callbacks::PrustiCompilerCalls;
use prusti_common::config;
use arg_value::arg_value;
use rustc_interface::interface::try_print_query_stack;
use log::info;

/// Link to report Prusti bugs
const BUG_REPORT_URL: &str = "https://github.com/viperproject/prusti-dev/issues/new";

lazy_static! {
    static ref ICE_HOOK: Box<dyn Fn(&panic::PanicInfo<'_>) + Sync + Send + 'static> = {
        let hook = panic::take_hook();
        panic::set_hook(box |info| report_prusti_ice(info, BUG_REPORT_URL));
        hook
    };
}

fn get_prusti_version_info() -> String {
    format!(
        "commit {} {}, built on {}",
        option_env!("COMMIT_HASH").unwrap_or("<unknown>"),
        option_env!("COMMIT_TIME").unwrap_or("<unknown>"),
        option_env!("BUILD_TIME").unwrap_or("<unknown>"),
    )
}

/// Report a readable error message in case of panic, with a link to open a new Prusti issue.
fn report_prusti_ice(info: &panic::PanicInfo<'_>, bug_report_url: &str) {
    // Invoke our ICE handler, which prints the actual panic message and optionally a backtrace
    (*ICE_HOOK)(info);

    // Separate the output with an empty line
    eprintln!();

    let emitter = box rustc_errors::emitter::EmitterWriter::stderr(
        rustc_errors::ColorConfig::Auto,
        None,
        false,
        false,
        None,
        false,
    );
    let handler = rustc_errors::Handler::with_emitter(true, None, emitter);

    // a .span_bug or .bug call has already printed what it wants to print.
    if !info.payload().is::<rustc_errors::ExplicitBug>() {
        let d = rustc_errors::Diagnostic::new(
            rustc_errors::Level::Bug, "unexpected panic"
        );
        handler.emit_diagnostic(&d);
    }

    let version_info = get_prusti_version_info();

    let xs: Vec<Cow<'static, str>> = vec![
        "Prusti or the compiler unexpectedly panicked. This is a bug.".into(),
        format!("We would appreciate a bug report: {}", bug_report_url).into(),
        format!("Prusti version: {}", version_info).into(),
    ];

    for note in &xs {
        handler.note_without_error(&note);
    }

    // If backtraces are enabled, also print the query stack
    let backtrace = env::var_os("RUST_BACKTRACE")
        .map_or(false, |x| &x != "0");

    if backtrace {
        try_print_query_stack(&handler, None);
    }
}

/// Initialize Prusti and the Rust compiler loggers.
fn init_loggers() {
    let env = env_logger::Env::new()
        .filter("PRUSTI_LOG")
        .write_style("PRUSTI_LOG_STYLE");
    env_logger::init_from_env(env);
    rustc_driver::init_rustc_env_logger();
}

const PRUSTI_PACKAGES: [&str; 4] = [
    "prusti-contracts-internal",
    "prusti-contracts-impl",
    "prusti-contracts",
    "prusti-specs",
];

fn main() {
    // We assume that prusti-rustc already removed the first "rustc" argument
    // added by RUSTC_WRAPPER and all command line arguments -P<arg>=<val>
    // have been filtered out.
    let mut rustc_args = config::get_filtered_args();

    // If the environment asks us to actually be rustc, or if lints have been disabled (which
    // indicates that an upstream dependency is being compiled), then run `rustc` instead of Prusti.
    let prusti_be_rustc = config::be_rustc();
    let are_lints_disabled = arg_value(&rustc_args, "--cap-lints", |val| val == "allow").is_some();
    let is_prusti_package = env::var("CARGO_PKG_NAME").map(|name| PRUSTI_PACKAGES.contains(&name.as_str())).unwrap_or(false);
    if prusti_be_rustc || are_lints_disabled || is_prusti_package {
        rustc_driver::main();
    }

    lazy_static::initialize(&ICE_HOOK);
    init_loggers();

    let exit_code = rustc_driver::catch_with_exit_code(move || {

        user::message(format!(
            "{}\n{}\n{}\n\n",
            r"  __          __        __  ___             ",
            r" |__)  _\/_  |__) |  | /__`  |   ____\/_  | ",
            r" |      /\   |  \ \__/ .__/  |       /\   | ",
        ));
        info!("Prusti version: {}", get_prusti_version_info());

        env::set_var("POLONIUS_ALGORITHM", "Naive");
        rustc_args.push("-Zborrowck=mir".to_owned());
        rustc_args.push("-Zpolonius".to_owned());
        rustc_args.push(format!(
            "-Znll-facts-dir={}",
            PathBuf::from(config::log_dir()).join("nll-facts").to_str()
                .expect("failed to configure nll-facts-dir")
        ));
        rustc_args.push("-Zidentify-regions".to_owned());
        rustc_args.push(format!(
            "-Zdump-mir-dir={}",
            PathBuf::from(config::log_dir()).join("mir").to_str()
                .expect("failed to configure dump-mir-dir")
        ));
        rustc_args.push("-Zalways-encode-mir".to_owned());
        rustc_args.push("-Zcrate-attr=feature(register_tool)".to_owned());
        rustc_args.push("-Zcrate-attr=register_tool(prusti)".to_owned());

        if config::check_overflows() {
            // Some crates might have a `overflow-checks = false` in their `Cargo.toml` to
            // disable integer overflow checks, but we want to ignore that.
            rustc_args.push("-Zforce-overflow-checks=yes".to_owned());
        }

        if config::dump_debug_info() {
            rustc_args.push("-Zdump-mir=all".to_owned());
            rustc_args.push("-Zdump-mir-graphviz".to_owned());
        }

        let mut callbacks = PrustiCompilerCalls::default();

        rustc_driver::RunCompiler::new(&rustc_args, &mut callbacks).run()
    });
    std::process::exit(exit_code)
}
