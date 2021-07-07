// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::{HashMap, HashSet};
use log::{debug, trace};
use rustc_hir as hir;
use rustc_middle::mir;
use rustc_hir::def_id::{DefId, LocalDefId};
use rustc_middle::ty;
use prusti_interface::environment::Environment;

/// Structure to collect closure instantiations annotated with `prusti::spec_only`.
pub struct SpecsClosuresCollector<'tcx> {
    visited: HashSet<LocalDefId>,
    /// For each instantiation of each closure: DefId, location, operands and types of operands.
    instantiations: HashMap<
        DefId,
        Vec<(
            DefId,
            mir::Location,
            Vec<mir::Operand<'tcx>>,
            Vec<ty::Ty<'tcx>>,
        )>,
    >,
}

impl<'tcx> SpecsClosuresCollector<'tcx> {
    pub fn new() -> Self {
        SpecsClosuresCollector {
            visited: HashSet::new(),
            instantiations: HashMap::new(),
        }
    }

    /// Collect instantiations of `prusti::spec_only` closures from all `prusti::spec_only` items.
    pub fn collect_from_all_spec_items(&mut self, env: &Environment<'tcx>) {
        debug!("Collecting closure instantiations...");
        let tcx = env.tcx();
        for &def_id in tcx.mir_keys(()).iter() {
            if env.has_prusti_attribute(def_id.to_def_id(), "spec_only") {
                self.collect(env, def_id);
            }
        }
    }

    /// Collect instantiations of `prusti::spec_only` closures from a given procedure.
    pub fn collect(&mut self, env: &Environment<'tcx>, def_id: LocalDefId) {
        debug!("Collecting closure instantiations in {:?}", def_id);
        // Avoid visiting the same procedure multiple times
        if self.visited.contains(&def_id) {
            debug!("ClosuresCollector already visited {:?} in the past.", def_id);
            return;
        } else {
            self.visited.insert(def_id);
        }
        let tcx = env.tcx();
        let mir = env.local_mir(def_id);

        // Iterate over all user-defined types and check which of them are user-defined closures.
        #[derive(Debug)]
        struct Instantiation<'tcx> {
            closure_def_id: DefId,
            location: Option<mir::Location>,
            operands: Vec<Option<mir::Operand<'tcx>>>,
            operand_tys: Vec<ty::Ty<'tcx>>,
        }

        let mut spec_closures = HashMap::new();
        for local in mir.vars_and_temps_iter() {
            let decl = &mir.local_decls[local];
            match decl.ty.kind() {
                ty::TyKind::Closure(cl_def_id, substs) => {
                    if env.has_prusti_attribute(*cl_def_id, "spec_only") {
                        let closure = substs.as_closure();
                        let operand_tys: Vec<_> = closure.upvar_tys().collect();
                        let instantiation = Instantiation {
                            closure_def_id: *cl_def_id,
                            location: None,
                            operands: vec![None; operand_tys.len()],
                            operand_tys,
                        };
                        spec_closures.insert(local, instantiation);
                    }
                }
                _=> {}
            }
        }


        for (bb_index, bb_data) in mir.basic_blocks().iter_enumerated() {
            for (stmt_index, stmt) in bb_data.statements.iter().enumerate() {
                match &stmt.kind {
                    mir::StatementKind::StorageLive(local) => {
                        if let Some(instantiation) = spec_closures.get_mut(&local) {
                            assert!(instantiation.location.is_none(), "location must not be set");
                        }
                    }
                    mir::StatementKind::StorageDead(local) => {
                        if let Some(instantiation) = spec_closures.get_mut(&local) {
                            assert!(instantiation.location.is_none(), "location is already set");
                            instantiation.location = Some(mir::Location {
                                block: bb_index,
                                statement_index: stmt_index,
                            });
                        }
                    }
                    mir::StatementKind::Assign(box (place, mir::Rvalue::Use(operand))) => {
                        if let Some(instantiation) = spec_closures.get_mut(&place.local) {
                            let mut indices = Vec::new();
                            for elem in place.projection.iter() {
                                match elem {
                                    mir::ProjectionElem::Field(field, _) => {
                                        indices.push(field.index());
                                    }
                                    _ => unreachable!(),
                                }
                            }
                            assert_eq!(indices.len(), 1);
                            let index = indices[0];
                            assert!(instantiation.operands[index].is_none());
                            instantiation.operands[index] = Some(operand.clone());
                        }
                    }
                    mir::StatementKind::Assign(box (place, rvalue)) => {
                        if let Some(_) = spec_closures.get_mut(&place.local) {
                            unreachable!("Unexpected rvalue: {:?}", rvalue);
                        }
                    }
                    _ => {}
                }
            }
        }

        for (_local, instantiation) in spec_closures {
            let Instantiation {
                closure_def_id,
                location,
                operands,
                operand_tys,
            } = instantiation;
            let location = location.unwrap();
            let operands: Vec<_> = operands.into_iter().map(Option::unwrap).collect();
            for (operand, &ty) in operands.iter().zip(&operand_tys) {
                assert_eq!(operand.ty(&*mir, tcx), ty);
            }
            let instantiations =
                self.instantiations.entry(closure_def_id).or_insert(vec![]);
            instantiations.push((
                def_id.to_def_id(),
                location,
                operands,
                operand_tys
            ));
        }
    }

    fn get_instantiations(&self, closure_def_id: DefId) -> Option<&[(
        DefId,
        mir::Location,
        Vec<mir::Operand<'tcx>>,
        Vec<ty::Ty<'tcx>>,
    )]> {
        trace!("Get closure instantiations for {:?}", closure_def_id);
        self.instantiations.get(&closure_def_id).map(|x| &x[..])
    }

    pub fn get_single_instantiation(&self, closure_def_id: DefId) -> Option<(
        DefId,
        mir::Location,
        Vec<mir::Operand<'tcx>>,
        Vec<ty::Ty<'tcx>>,
    )> {
        self.get_instantiations(closure_def_id).and_then(|instantiations| {
            assert!(instantiations.len() <= 1);
            instantiations.get(0).cloned()
        })
    }
}
