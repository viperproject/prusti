use log::{debug, trace};
use prusti_common::config;
use rustc_hir as hir;
use rustc_hir::{def_id::DefId, itemlikevisit::ItemLikeVisitor};
use rustc_middle::{mir, ty, ty::TyCtxt};
use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, BufWriter, Write},
    path::PathBuf,
};
use rustc_infer::infer::{InferCtxt, TyCtxtInferExt};

use crate::environment::borrowck2::obtain_mir_body;

pub(super) fn dump_lifetime_info<'tcx>(tcx: TyCtxt<'tcx>) {
    trace!("[dump_lifetime_info] enter");

    // Collect all procedures.
    let mut visitor = ProcedureCollector {
        tcx,
        procedures: Vec::new(),
    };
    tcx.hir().krate().visit_all_item_likes(&mut visitor);

    // Print info about each procedure.
    for def_id in visitor.procedures {
        // eprintln!("def_id: {:?}", def_id);
        print_info(tcx, def_id);
        // let outlives = gather_outlives(tcx.param_env(def_id));
        // eprintln!("outlives: {:?}", outlives);
        // eprintln!("inferred_outlives_of: {:?}", tcx.inferred_outlives_of(def_id));
        // eprintln!("implied_outlives_bounds: {:?}", tcx.implied_outlives_bounds(def_id));
    }

    trace!("[dump_lifetime_info] exit");
}

// fn gather_outlives(param_env: ty::ParamEnv) -> HashMap<ty::Region, HashSet<ty::Region>> {
//     let mut result = HashMap::<ty::Region, HashSet<ty::Region>>::new();
//     let outlives = param_env.caller_bounds().iter()
//         .filter_map(|b| match b.kind().skip_binder() {
//             ty::PredicateKind::RegionOutlives(ty::OutlivesPredicate(a, b)) => Some((a, b)),
//             _ => None
//         });
//     for (a, b) in outlives {
//         result.entry(a).or_default().insert(b);
//     }
//     result
// }

struct ProcedureCollector<'tcx> {
    tcx: TyCtxt<'tcx>,
    procedures: Vec<DefId>,
}

impl<'tcx> ItemLikeVisitor<'tcx> for ProcedureCollector<'tcx> {
    fn visit_item(&mut self, item: &hir::Item) {
        if let hir::ItemKind::Fn(..) = item.kind {
            let def_id = self.tcx.hir().local_def_id(item.hir_id()).to_def_id();
            self.procedures.push(def_id);
        }
    }

    fn visit_trait_item(&mut self, trait_item: &hir::TraitItem) {
        // Skip associated types and other non-methods items
        if let hir::TraitItemKind::Fn(..) = trait_item.kind {
            // continue
        } else {
            return;
        }
        // Skip body-less trait methods
        if let hir::TraitItemKind::Fn(_, hir::TraitFn::Required(_)) = trait_item.kind {
            return;
        }
        let def_id = self.tcx.hir().local_def_id(trait_item.hir_id()).to_def_id();
        self.procedures.push(def_id);
    }

    fn visit_impl_item(&mut self, impl_item: &hir::ImplItem) {
        // Skip associated types and other non-methods items
        if let hir::ImplItemKind::Fn(..) = impl_item.kind {
            // continue
        } else {
            return;
        }
        let def_id = self.tcx.hir().local_def_id(impl_item.hir_id()).to_def_id();
        self.procedures.push(def_id);
    }

    fn visit_foreign_item(&mut self, _foreign_item: &hir::ForeignItem) {
        // Nothing
    }
}

/// Print info about the procedure with the given `def_id`.
fn print_info<'tcx>(tcx: TyCtxt<'tcx>, def_id: DefId) {
    trace!("[print_info] enter {:?}", def_id);

    let body = obtain_mir_body(tcx, def_id);

    let local_def_id = def_id.expect_local();
    let def_path = tcx.hir().def_path(local_def_id);
    let output_path = PathBuf::from(config::log_dir())
        .join("nll-facts")
        .join(def_path.to_filename_friendly_no_crate())
        .join("output_lifetime.dot");
    debug!("Writing output to {:?}", output_path);
    body.to_graphviz(&output_path).unwrap();

    trace!("[print_info] exit {:?}", def_id);
}
