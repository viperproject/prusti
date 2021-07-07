#![feature(rustc_private)]

/// Source: https://github.com/rust-lang/miri/blob/master/benches/helpers/miri_helper.rs

extern crate rustc_ast;
extern crate rustc_middle;
extern crate rustc_hir;
extern crate rustc_driver;
extern crate rustc_interface;
extern crate rustc_session;

use rustc_ast::ast;
use rustc_middle::ty;
use rustc_hir::def_id::DefId;
use rustc_driver::Compilation;
use rustc_interface::{interface, Queries};
use rustc_session::Attribute;

use analysis::Analyzer;
use analysis::abstract_domains::{ReachingDefsState, DefinitelyInitializedState};

struct OurCompilerCalls {
    args: Vec<String>,
}

fn get_attribute<'tcx>(tcx: ty::TyCtxt<'tcx>, def_id: DefId, segment1: &str, segment2: &str) -> Option<&'tcx Attribute> {
    tcx.get_attrs(def_id).iter().find(|attr| match &attr.kind {
        ast::AttrKind::Normal(ast::AttrItem {
                                  path: ast::Path { span: _, segments, tokens: _ },
                                  args: ast::MacArgs::Empty,
                                  tokens: _,
                              }, _) => {
            segments.len() == 2
                && segments[0].ident.as_str() == segment1
                && segments[1].ident.as_str() == segment2
        }
        _ => false,
    }
    )
}

impl rustc_driver::Callbacks for OurCompilerCalls {
    fn after_analysis<'tcx>(
        &mut self,
        compiler: &interface::Compiler,
        queries: &'tcx Queries<'tcx>,
    ) -> Compilation {
        compiler.session().abort_if_errors();

        let abstract_domain: &str = self.args.iter()
            .filter(|a| a.starts_with("--ADdomain"))
            .flat_map(|a| a.rsplit("="))
            .next()
            .unwrap();

        println!("Analyzing file {} using {}...", compiler.input().source_name().prefer_local(), abstract_domain);

        queries.global_ctxt().unwrap().peek_mut().enter(|tcx| {

            // collect all functions with attribute #[analyzer::run]
            let mut local_def_ids: Vec<_> = tcx.mir_keys(()).iter()
                .filter(|id| get_attribute(tcx, id.to_def_id(), "analyzer", "run").is_some())
                .collect();

            // sort according to argument span to ensure deterministic output
            local_def_ids.sort_unstable_by_key(|id| get_attribute(tcx, id.to_def_id(), "analyzer", "run").unwrap().span);

            let analyzer = Analyzer::new(tcx);

            for &local_def_id in local_def_ids {
                println!("Result for function {}():", tcx.item_name(local_def_id.to_def_id()));

                let body = tcx.optimized_mir(local_def_id);

                match abstract_domain {
                    "ReachingDefsState" => {
                        let result = analyzer.run_fwd_analysis::<ReachingDefsState>(&body);
                        match result {
                            Ok(state) => print!("{}", serde_json::to_string_pretty(&state).unwrap()),
                            Err(e) => eprintln!("{}", e.to_pretty_str(&body))
                        }
                    },
                    "DefinitelyInitializedState" => {
                        let result = analyzer.run_fwd_analysis::<DefinitelyInitializedState>(&body);
                        match result {
                            Ok(state) => print!("{}", serde_json::to_string_pretty(&state).unwrap()),
                            Err(e) => eprintln!("{}", e.to_pretty_str(&body))
                        }
                    },
                    _ => panic!("Unknown domain argument: {}", abstract_domain)
                }
            }
        });

        compiler.session().abort_if_errors();

        Compilation::Stop
    }
}

/// Run an analysis by calling like it rustc
///
/// Give arguments to the analyzer by prefixing them with '--AD'
/// A abstract domain has to be provided by using '--ADdomain=' (without spaces), e.g.:
/// --ADdomain=ReachingDefsState or --ADdomain=DefinitelyInitializedState
fn main() {
    let mut compiler_args= Vec::new();
    let mut callback_args= Vec::new();
    for arg in std::env::args() {
        if arg.starts_with("--AD") {
            callback_args.push(arg);
        }
        else {
            compiler_args.push(arg);
        }
    }

    compiler_args.push("-Zcrate-attr=feature(register_tool)".to_owned());
    compiler_args.push("-Zcrate-attr=register_tool(analyzer)".to_owned());

    let mut callbacks = OurCompilerCalls { args: callback_args };
    // Invoke compiler, and handle return code.
    let exit_code = rustc_driver::catch_with_exit_code(move || {
        rustc_driver::RunCompiler::new(&compiler_args, &mut callbacks).run()
    });
    std::process::exit(exit_code)
}
