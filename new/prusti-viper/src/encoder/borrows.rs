// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::encoder::places;
use prusti_interface::data::ProcedureDefId;
// use prusti_interface::specifications::{
//     AssertionKind, SpecificationSet, TypedAssertion, TypedExpression, TypedSpecification,
//     TypedSpecificationSet,
// };
use rustc_hir::{self as hir, Mutability};
use rustc_middle::mir;
use rustc_middle::ty::{self, Ty, TyCtxt};
// use rustc_data_structures::indexed_vec::Idx;
use std::collections::HashMap;
use std::fmt;
use crate::utils::type_visitor::{self, TypeVisitor};
use prusti_interface::specs::typed;
use log::trace;

#[derive(Clone, Debug)]
pub struct BorrowInfo<P>
where
    P: fmt::Debug,
{
    /// Region of this borrow. None means static.
    pub region: Option<ty::BoundRegion>,
    pub blocking_paths: Vec<(P, Mutability)>,
    pub blocked_paths: Vec<(P, Mutability)>,
    //blocked_lifetimes: Vec<String>, TODO: Get this info from the constraints graph.
}

impl<P: fmt::Debug> BorrowInfo<P> {
    fn new(region: Option<ty::BoundRegion>) -> Self {
        BorrowInfo {
            region,
            blocking_paths: Vec::new(),
            blocked_paths: Vec::new(),
        }
    }
}

impl<P: fmt::Debug> fmt::Display for BorrowInfo<P> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let lifetime = match self.region {
            None => format!("static"),
            Some(ty::BoundRegion::BrAnon(id)) => format!("#{}", id),
            Some(ty::BoundRegion::BrNamed(_, name)) => name.to_string(),
            _ => unimplemented!(),
        };
        writeln!(f, "BorrowInfo<{}> {{", lifetime)?;
        for path in self.blocking_paths.iter() {
            writeln!(f, "  {:?}", path)?;
        }
        writeln!(f, "  --*")?;
        for path in self.blocked_paths.iter() {
            writeln!(f, "  {:?}", path)?;
        }
        writeln!(f, "}}")
    }
}

/// Contract of a specific procedure. It is a separate struct from a
/// general procedure info because we want to be able to translate
/// procedure calls before translating call targets.
/// TODO: Move to some properly named module.
#[derive(Clone, Debug)]
pub struct ProcedureContractGeneric<'tcx, L, P>
where
    L: fmt::Debug,
    P: fmt::Debug,
{
    /// Formal arguments for which we should have permissions in the
    /// precondition. This includes both borrows and moved in values.
    /// For example, if `_2` is in the vector, this means that we have
    /// `T(_2)` in the precondition.
    pub args: Vec<L>,
    /// Borrowed arguments that are directly returned to the caller (not via
    /// a magic wand). For example, if `*(_2.1).0` is in the vector, this
    /// means that we have `T(old[precondition](_2.1.ref.0))` in the
    /// postcondition. It also includes information about the mutability
    /// of the original reference.
    pub returned_refs: Vec<(P, Mutability)>,
    /// The returned value for which we should have permission in
    /// the postcondition.
    pub returned_value: L,
    /// Magic wands passed out of the procedure.
    /// TODO: Implement support for `blocked_lifetimes` via nested magic wands.
    pub borrow_infos: Vec<BorrowInfo<P>>,
    /// The functional specification: precondition and postcondition
    pub specification: typed::SpecificationSet<'tcx>,
}

impl<L: fmt::Debug, P: fmt::Debug> ProcedureContractGeneric<'_, L, P> {
    pub fn functional_precondition(&self) -> &[typed::Assertion] {
        if let typed::SpecificationSet::Procedure(spec) = &self.specification {
            &spec.pres
        } else {
            unreachable!("Unexpected: {:?}", self.specification)
        }
    }

    pub fn functional_postcondition(&self) -> &[typed::Assertion] {
        if let typed::SpecificationSet::Procedure(spec) = &self.specification {
            &spec.posts
        } else {
            unreachable!("Unexpected: {:?}", self.specification)
        }
    }

//     pub fn pledges(&self) -> Vec<(Option<TypedExpression>, TypedAssertion, TypedAssertion)> {
//         let mut pledges = Vec::new();
//         fn check_assertion(
//             assertion: &TypedAssertion,
//             pledges: &mut Vec<(Option<TypedExpression>, TypedAssertion, TypedAssertion)>,
//         ) {
//             match assertion.kind.as_ref() {
//                 AssertionKind::Expr(_)
//                 | AssertionKind::Implies(_, _)
//                 | AssertionKind::TypeCond(_, _)
//                 | AssertionKind::ForAll(_, _, _) => {}
//                 AssertionKind::And(ref assertions) => {
//                     for assertion in assertions {
//                         check_assertion(assertion, pledges);
//                     }
//                 }
//                 AssertionKind::Pledge(ref reference, ref lhs, ref rhs) => {
//                     pledges.push((reference.clone(), lhs.clone(), rhs.clone()));
//                 }
//             };
//         }
//         for item in self.functional_postcondition() {
//             check_assertion(&item.assertion, &mut pledges);
//         }
//         pledges
//     }
}

/// Procedure contract as it is defined in MIR.
pub type ProcedureContractMirDef<'tcx> = ProcedureContractGeneric<'tcx, mir::Local, mir::Place<'tcx>>;

/// Specialized procedure contract for use in translation.
pub type ProcedureContract<'tcx> = ProcedureContractGeneric<'tcx, places::Local, places::Place<'tcx>>;

impl<L: fmt::Debug, P: fmt::Debug> fmt::Display for ProcedureContractGeneric<'_, L, P> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "ProcedureContract {{")?;
        writeln!(f, "IN:")?;
        for path in self.args.iter() {
            writeln!(f, "  {:?}", path)?;
        }
        writeln!(f, "OUT:")?;
        for path in self.returned_refs.iter() {
            writeln!(f, "  {:?}", path)?;
        }
        writeln!(f, "MAGIC:")?;
        for borrow_info in self.borrow_infos.iter() {
            writeln!(f, "{}", borrow_info)?;
        }
        writeln!(f, "}}")
    }
}

// fn get_place_root<'tcx>(place: &mir::Place<'tcx>) -> mir::Local {
//     match place {
//         &mir::Place::Local(local) => local,
//         &mir::Place::Projection(ref projection) => get_place_root(&projection.base),
//         _ => unimplemented!(),
//     }
// }

impl<'tcx> ProcedureContractMirDef<'tcx> {
    /// Specialize to the definition site contract.
    pub fn to_def_site_contract(&self) -> ProcedureContract<'tcx> {
        let borrow_infos = self
            .borrow_infos
            .iter()
            .map(|info| BorrowInfo {
                region: info.region,
                blocking_paths: info
                    .blocking_paths
                    .iter()
                    .map(|(p, m)| (p.into(), *m))
                    .collect(),
                blocked_paths: info
                    .blocked_paths
                    .iter()
                    .map(|(p, m)| (p.into(), *m))
                    .collect(),
            })
            .collect();
        ProcedureContract {
            args: self.args.iter().map(|&a| a.into()).collect(),
            returned_refs: self
                .returned_refs
                .iter()
                .map(|(r, m)| (r.into(), *m))
                .collect(),
            returned_value: self.returned_value.into(),
            borrow_infos,
            specification: self.specification.clone(),
        }
    }

//     /// Specialize to the call site contract.
//     pub fn to_call_site_contract(
//         &self,
//         args: &Vec<places::Local>,
//         target: places::Local,
//     ) -> ProcedureContract<'tcx> {
//         assert_eq!(self.args.len(), args.len());
//         let mut substitutions = HashMap::new();
//         substitutions.insert(self.returned_value, target);
//         for (from, to) in self.args.iter().zip(args) {
//             substitutions.insert(*from, *to);
//         }
//         let substitute = |(place, mutability): &(_, Mutability)| {
//             let root = &get_place_root(place);
//             let substitute_place = places::Place::SubstitutedPlace {
//                 substituted_root: *substitutions.get(root).unwrap(),
//                 place: place.clone(),
//             };
//             (substitute_place, *mutability)
//         };
//         let borrow_infos = self
//             .borrow_infos
//             .iter()
//             .map(|info| BorrowInfo {
//                 region: info.region,
//                 blocking_paths: info.blocking_paths.iter().map(&substitute).collect(),
//                 blocked_paths: info.blocked_paths.iter().map(&substitute).collect(),
//             })
//             .collect();
//         let returned_refs = self.returned_refs.iter().map(&substitute).collect();
//         let result = ProcedureContract {
//             args: args.clone(),
//             returned_refs: returned_refs,
//             returned_value: target,
//             borrow_infos,
//             specification: self.specification.clone(),
//         };
//         result
//     }
}

pub struct BorrowInfoCollectingVisitor<'tcx> {
    borrow_infos: Vec<BorrowInfo<mir::Place<'tcx>>>,
    /// References that were passed as arguments. We are interested only in
    /// references that can be blocked.
    references_in: Vec<(mir::Place<'tcx>, Mutability)>,
    tcx: TyCtxt<'tcx>,
    /// Can the currently analysed path block other paths? For return
    /// type this is initially true, and for parameters it is true below
    /// the first reference.
    is_path_blocking: bool,
    current_path: Option<mir::Place<'tcx>>,
}

impl<'tcx> BorrowInfoCollectingVisitor<'tcx> {
    fn new(tcx: TyCtxt<'tcx>) -> Self {
        BorrowInfoCollectingVisitor {
            borrow_infos: Vec::new(),
            references_in: Vec::new(),
            tcx,
            is_path_blocking: false,
            current_path: None,
        }
    }

    fn analyse_return_ty(&mut self, ty: Ty<'tcx>) {
        self.is_path_blocking = true;
        self.current_path = Some(mir::RETURN_PLACE.into());
        self.visit_ty(ty);
        self.current_path = None;
    }

    fn analyse_arg(&mut self, arg: mir::Local, ty: Ty<'tcx>) {
        self.is_path_blocking = false;
        self.current_path = Some(arg.into());
        self.visit_ty(ty);
        self.current_path = None;
    }

//     fn extract_bound_region(&self, region: ty::Region<'tcx>) -> Option<ty::BoundRegion> {
//         match region {
//             &ty::RegionKind::ReFree(free_region) => Some(free_region.bound_region),
//             // TODO: is this correct?!
//             &ty::RegionKind::ReLateBound(_, bound_region) => Some(bound_region),
//             &ty::RegionKind::ReEarlyBound(early_region) => Some(early_region.to_bound_region()),
//             &ty::RegionKind::ReStatic => None,
//             &ty::RegionKind::ReScope(_scope) => None, //  FIXME: This is incorrect.
//             x => unimplemented!("{:?}", x),
//         }
//     }

//     fn get_or_create_borrow_info(
//         &mut self,
//         region: Option<ty::BoundRegion>,
//     ) -> &mut BorrowInfo<mir::Place<'tcx>> {
//         if let Some(index) = self
//             .borrow_infos
//             .iter()
//             .position(|info| info.region == region)
//         {
//             &mut self.borrow_infos[index]
//         } else {
//             let borrow_info = BorrowInfo::new(region);
//             self.borrow_infos.push(borrow_info);
//             self.borrow_infos.last_mut().unwrap()
//         }
//     }
}

impl<'tcx> TypeVisitor<'tcx> for BorrowInfoCollectingVisitor<'tcx> {
    fn tcx(&self) -> TyCtxt<'tcx> {
        self.tcx
    }

//     fn visit_field(
//         &mut self,
//         index: usize,
//         field: &ty::FieldDef,
//         substs: &'tcx ty::subst::Substs<'tcx>,
//     ) {
//         trace!("visit_field({}, {:?})", index, field);
//         let old_path = self.current_path.take().unwrap();
//         let ty = field.ty(self.tcx(), substs);
//         let field_id = mir::Field::new(index);
//         self.current_path = Some(old_path.clone().field(field_id, ty));
//         type_visitor::walk_field(self, field, substs);
//         self.current_path = Some(old_path);
//     }

//     fn visit_ref(
//         &mut self,
//         region: ty::Region<'tcx>,
//         ty: ty::Ty<'tcx>,
//         mutability: hir::Mutability,
//     ) {
//         trace!(
//             "visit_ref({:?}, {:?}, {:?}) current_path={:?}",
//             region,
//             ty,
//             mutability,
//             self.current_path
//         );
//         let bound_region = self.extract_bound_region(region);
//         let is_path_blocking = self.is_path_blocking;
//         let old_path = self.current_path.take().unwrap();
//         let current_path = old_path.clone().deref();
//         self.current_path = Some(current_path.clone());
//         let borrow_info = self.get_or_create_borrow_info(bound_region);
//         if is_path_blocking {
//             borrow_info.blocking_paths.push((current_path, mutability));
//         } else {
//             borrow_info
//                 .blocked_paths
//                 .push((current_path.clone(), mutability));
//             self.references_in.push((current_path, mutability));
//         }
//         self.is_path_blocking = true;
//         //type_visitor::walk_ref(self, region, ty, mutability);
//         self.is_path_blocking = is_path_blocking;
//         self.current_path = Some(old_path);
//     }

//     fn visit_raw_ptr(&mut self, ty: ty::Ty<'tcx>, mutability: hir::Mutability) {
//         trace!(
//             "visit_raw_ptr({:?}, {:?}) current_path={:?}",
//             ty,
//             mutability,
//             self.current_path
//         );
//         // TODO
//         debug!("BorrowInfoCollectingVisitor::visit_raw_ptr is unimplemented");
//     }
}

pub fn compute_procedure_contract<'p, 'a, 'tcx>(
    proc_def_id: ProcedureDefId,
    tcx: TyCtxt<'tcx>,
    specification: typed::SpecificationSet<'tcx>,
    maybe_tymap: Option<&HashMap<ty::Ty<'tcx>, ty::Ty<'tcx>>>,
) -> ProcedureContractMirDef<'tcx>
where
    'a: 'p,
    'tcx: 'a,
{
    trace!("[compute_borrow_infos] enter name={:?}", proc_def_id);

    let fn_sig = tcx.fn_sig(proc_def_id);
    trace!("fn_sig: {:?}", fn_sig);

    let mut fake_mir_args = Vec::new();
    let mut fake_mir_args_ty = Vec::new();

    // FIXME; "skip_binder" is most likely wrong
    for i in 0usize..fn_sig.inputs().skip_binder().len() {
        fake_mir_args.push(mir::Local::from_usize(i + 1));
        let arg_ty = fn_sig.input(i);
        let arg_ty = arg_ty.skip_binder();
        let ty = if let Some(replaced_arg_ty) = maybe_tymap.and_then(|tymap| tymap.get(arg_ty)) {
            replaced_arg_ty.clone()
        } else {
            arg_ty.clone()
        };
        fake_mir_args_ty.push(ty);
    }
    let return_ty = fn_sig.output().skip_binder().clone();

    let mut visitor = BorrowInfoCollectingVisitor::new(tcx);
    for (arg, arg_ty) in fake_mir_args.iter().zip(fake_mir_args_ty) {
        visitor.analyse_arg(*arg, arg_ty);
    }
    visitor.analyse_return_ty(return_ty);
    let borrow_infos: Vec<_> = visitor
        .borrow_infos
        .into_iter()
        .filter(|info| !info.blocked_paths.is_empty() && !info.blocking_paths.is_empty())
        .collect();
    let is_not_blocked = |place: &mir::Place<'tcx>| {
        !borrow_infos.iter().any(|info| {
            info.blocked_paths
                .iter()
                .any(|(blocked_place, _)| blocked_place == place)
        })
    };
    let returned_refs: Vec<_> = visitor
        .references_in
        .into_iter()
        .filter(|(place, _)| is_not_blocked(place))
        .collect();
    let contract = ProcedureContractGeneric {
        args: fake_mir_args,
        returned_refs,
        returned_value: mir::RETURN_PLACE,
        borrow_infos,
        specification,
    };

    trace!("[compute_borrow_infos] exit result={}", contract);
    contract
}
