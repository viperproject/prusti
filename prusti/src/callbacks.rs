use prusti_interface::{specs, environment::{Environment, mir_storage}};
use rustc_driver::Compilation;
use rustc_hir::intravisit;
use rustc_interface::interface::Compiler;
use rustc_interface::{Queries, Config};
use regex::Regex;
use prusti_common::config;
use crate::verifier::verify;
use rustc_middle::ty::query::query_values::mir_borrowck;
use rustc_middle::ty::query::Providers;
use rustc_session::Session;
use rustc_hir::def_id::LocalDefId;
use rustc_middle::ty::{self, TyCtxt};

#[derive(Default)]
pub struct PrustiCompilerCalls;

fn mir_borrowck<'tcx>(tcx: TyCtxt<'tcx>, def_id: LocalDefId) -> mir_borrowck<'tcx> {
    let body_with_facts = rustc_mir::consumers::get_body_with_borrowck_facts(
        tcx, ty::WithOptConstParam::unknown(def_id));
    // SAFETY: This is safe because we are feeding in the same `tcx` that is
    // going to be used as a witness when pulling out the data.
    unsafe { mir_storage::store_mir_body(tcx, def_id, body_with_facts); }
    let mut providers = Providers::default();
    rustc_mir::provide(&mut providers);
    let original_mir_borrowck = providers.mir_borrowck;
    original_mir_borrowck(tcx, def_id)
}

fn override_queries(_session: &Session, local: &mut Providers, external: &mut Providers) {
    local.mir_borrowck = mir_borrowck;
    external.mir_borrowck = mir_borrowck;
}

impl rustc_driver::Callbacks for PrustiCompilerCalls {
    fn config(&mut self, config: &mut Config) {
        assert!(config.override_queries.is_none());
        config.override_queries = Some(override_queries);
    }
    fn after_expansion<'tcx>(
        &mut self,
        compiler: &Compiler,
        queries: &'tcx Queries<'tcx>,
    ) -> Compilation {
        compiler.session().abort_if_errors();
        let (krate, _resolver, _lint_store) = &mut *queries.expansion().unwrap().peek_mut();
        if config::print_desugared_specs() {
            rustc_driver::pretty::print_after_parsing(
                compiler.session(),
                compiler.input(),
                krate,
                rustc_session::config::PpMode::Source(
                    rustc_session::config::PpSourceMode::Normal,
                ),
                None,
            );
        }
        Compilation::Continue
    }
    fn after_analysis<'tcx>(
        &mut self,
        compiler: &Compiler,
        queries: &'tcx Queries<'tcx>,
    ) -> Compilation {
        compiler.session().abort_if_errors();
        queries.global_ctxt().unwrap().peek_mut().enter(|tcx| {
            let hir = tcx.hir();
            let krate = hir.krate();
            let env = Environment::new(tcx);
            let mut spec_checker = specs::checker::SpecChecker::new();
            spec_checker.check_predicate_usages(tcx, krate);
            spec_checker.report_errors(&env);
            compiler.session().abort_if_errors();

            let mut spec_collector = specs::SpecCollector::new(&env);
            intravisit::walk_crate(&mut spec_collector, &krate);
            let def_spec = spec_collector.build_def_specs(&env);
            if config::print_typeckd_specs() {
                let mut values: Vec<_> = def_spec
                    .specs
                    .values()
                    .map(|spec| format!("{:?}", spec))
                    .collect();
                if config::hide_uuids() {
                    let uuid = Regex::new(
                        "[a-z0-9]{8}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{4}-[a-z0-9]{12}"
                    ).unwrap();
                    let num_uuid = Regex::new("[a-z0-9]{32}").unwrap();
                    let mut replaced_values: Vec<String> = vec![];
                    for item in values {
                        let item = num_uuid.replace_all(&item, "$(NUM_UUID)");
                        let item = uuid.replace_all(&item, "$(UUID)");
                        replaced_values.push(String::from(item));
                    }
                    values = replaced_values;
                }
                // We sort in this strange way so that the output is
                // determinstic enough to be used in tests.
                values.sort_by_key(|v| (v.len(), v.to_string()));
                for value in values {
                    println!("{}", value);
                }
            }
            if !config::no_verify() {
                verify(env, def_spec);
            }
        });

        compiler.session().abort_if_errors();
        if config::full_compilation() {
            Compilation::Continue
        } else {
            Compilation::Stop
        }
    }
}