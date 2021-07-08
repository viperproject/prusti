use log::trace;
use rustc_middle::{mir, ty, ty::TyCtxt};
use rustc_hir::{def_id::DefId};
use rustc_mir::borrow_check::universal_regions::UniversalRegions;

mod compiler;
mod graphviz;

pub use self::compiler::MirBody;

/// Obtain a `MirBody` for verification.
pub fn obtain_mir_body<'tcx>(tcx: TyCtxt<'tcx>, def_id: DefId) -> MirBody<'tcx> {
    trace!("[obtain_mir_body] enter {:?}", def_id);
    let input_body = tcx.optimized_mir(def_id);
    let body = self::compiler::enrich_mir_body(tcx, def_id, input_body);
    trace!("[obtain_mir_body] exit {:?}", def_id);
    body
}