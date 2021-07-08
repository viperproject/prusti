//! An interface to the compiler.
// Most code here are copied from the Rust compiler sources. See its license for
// details.

use rustc_hir::{def_id::DefId};
use rustc_middle::{mir, ty, ty::TyCtxt};
use rustc_middle::ty::RegionVid;
use rustc_infer::infer::{InferCtxt, TyCtxtInferExt};
use rustc_mir::borrow_check::universal_regions::UniversalRegions;
use rustc_mir::borrow_check::renumber::renumber_mir;
use rustc_index::vec::IndexVec;
use std::rc::Rc;
use rustc_mir::borrow_check::type_check::free_region_relations::CreateResult;
use rustc_mir::borrow_check::member_constraints::MemberConstraintSet;
use rustc_mir::borrow_check::type_check::MirTypeckRegionConstraints;
use rustc_mir::borrow_check::constraints::OutlivesConstraintSet;
use rustc_mir::borrow_check::region_infer::values::LivenessValues;
use rustc_mir::borrow_check::region_infer::values::PlaceholderIndices;
use rustc_mir::borrow_check::type_check::free_region_relations;
use rustc_mir::borrow_check::region_infer::values::RegionValueElements;
use rustc_mir::borrow_check::location::LocationTable;
use rustc_mir::borrow_check::borrow_set::BorrowSet;
use rustc_mir::borrow_check::type_check::type_check;
use rustc_mir::dataflow::impls::MaybeInitializedPlaces;
use rustc_mir::dataflow::MoveDataParamEnv;
use rustc_mir::dataflow::move_paths::MoveData;
use rustc_middle::mir::Place;
use rustc_mir::dataflow::move_paths::MoveError;
use rustc_mir::borrow_check::facts::AllFacts;
use rustc_mir::dataflow::Analysis;
use rustc_mir::borrow_check::Upvar;
use polonius_engine as polonius;
use rustc_mir::borrow_check::nll::PoloniusOutput;
use rustc_mir::borrow_check::nll;

/// Enrich the given `mir::Body` with the lifetime information.
pub(in crate::environment::borrowck2) fn enrich_mir_body<'tcx>(
    tcx: TyCtxt<'tcx>,
    def_id: DefId,
    input_body: &mir::Body<'tcx>
) -> super::MirBody<'tcx> {
    let mut result = None;
    let result_borrow = &mut result;
    tcx.infer_ctxt().enter(|infcx| {
        *result_borrow = Some(
            collect_borrowck_info(&infcx, input_body),
        );
    });
    let (body, polonius_input_facts, polonius_output_facts, location_table) = result.unwrap();
    let local_names = super::derive::extract_local_names(&body);
    let outlives = super::derive::compute_outlives_map(&polonius_input_facts.outlives);
    let lifetimes = super::compute_lifetimes::compute_lifetimes(
        &polonius_input_facts,
        &polonius_output_facts,
        &location_table,
    );
    super::MirBody {
        def_id,
        body,
        tcx,
        polonius_input_facts,
        polonius_output_facts,
        location_table,
        local_names,
        outlives,
        lifetimes,
    }
}

fn collect_borrowck_info<'tcx>(
    infcx: &InferCtxt<'_, 'tcx>,
    input_body: &mir::Body<'tcx>,
) -> (mir::Body<'tcx>, AllFacts, PoloniusOutput, LocationTable) {
    let tcx = infcx.tcx;

    let mut body = input_body.clone();

    let def = input_body.source.with_opt_param().as_local().unwrap();
    let param_env = tcx.param_env(def.did);
    let mut promoted = IndexVec::new();
                    // TODO: How to get promoted for MIR optimized? One idea
                    // would be to try overriding mir_promoted query and saving
                    // information about the promoted constants.
    let free_regions = nll::replace_regions_in_mir(infcx, param_env, &mut body, &mut promoted);

    let location_table = LocationTable::new(&body);
    let (move_data, _move_errors): (MoveData<'tcx>, Vec<(Place<'tcx>, MoveError<'tcx>)>) =
    match MoveData::gather_moves(&body, tcx, param_env) {
        Ok(move_data) => (move_data, Vec::new()),
        Err((move_data, move_errors)) => (move_data, move_errors),
    };
    let mdpe = MoveDataParamEnv { move_data, param_env };
    let id = tcx.hir().local_def_id_to_hir_id(def.did);
    let locals_are_invalidated_at_exit = tcx.hir().body_owner_kind(id).is_fn_or_closure();
    let borrow_set = BorrowSet::build(tcx, &body, locals_are_invalidated_at_exit, &mdpe.move_data);
    let mut flow_inits = MaybeInitializedPlaces::new(tcx, &body, &mdpe)
        .into_engine(tcx, &body)
        .pass_name("borrowck")
        .iterate_to_fixpoint()
        .into_results_cursor(&body);
    let tables = tcx.typeck_opt_const_arg(def);
    if let Some(ErrorReported) = tables.tainted_by_errors {
        infcx.set_tainted_by_errors();
    }
    let upvars: Vec<_> = tables
        .closure_min_captures_flattened(def.did.to_def_id())
        .map(|captured_place| {
            let capture = captured_place.info.capture_kind;
            let by_ref = match capture {
                ty::UpvarCapture::ByValue(_) => false,
                ty::UpvarCapture::ByRef(..) => true,
            };
            Upvar { place: captured_place.clone(), by_ref }
        })
        .collect();

    let nll::NllOutput {
        regioncx: _,
        opaque_type_values: _,
        polonius_input,
        polonius_output,
        opt_closure_req: _,
        nll_errors,
    } = nll::compute_regions(
        infcx,
        free_regions,
        &body,
        &promoted,
        &location_table,
        param_env,
        &mut flow_inits,
        &mdpe.move_data,
        &borrow_set,
        &upvars,
    );
    assert!(nll_errors.is_empty());

    // TODO: Change unwraps into returning error.
    (body,
        *polonius_input.unwrap(), std::rc::Rc::try_unwrap(polonius_output.unwrap()).unwrap(), location_table)
}