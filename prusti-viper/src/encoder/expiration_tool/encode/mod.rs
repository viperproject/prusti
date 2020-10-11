use std::cell::Ref;

use binding::Binding;
use binding::LiftBindings;
use prusti_common::vir;
use rustc_hir::def_id::DefId;
use rustc_hir::def_id::LocalDefId;
use rustc_middle::mir;
use rustc_middle::ty;

use crate::encoder::borrows::ProcedureContract;
use crate::encoder::procedure_encoder::ProcedureEncoder;

mod expression;
mod package;
mod expiration;
mod common;
mod pledge;
pub mod binding;
mod utils;

struct ExpirationToolEncoder<'a, 'p, 'v: 'p, 'tcx: 'v> {
    procedure_encoder: &'a mut ProcedureEncoder<'p, 'v, 'tcx>,
    /// The contract of the procedure we're encoding the expiration tool for.
    contract: &'a ProcedureContract<'tcx>,
    /// If we encode an expiration tool for a function call, this location should point to the
    /// function call statement. If we encode an expiration for a function itself, this location
    /// should be `None`.
    call_location: Option<mir::Location>,
    /// If we encode an expiration tool for a function, this location should point to the return
    /// statement of the function. If we encode an expiration for a function call, this location
    /// should be `None`.
    return_location: Option<mir::Location>,
    /// The function arguments, encoded as Viper expressions.
    encoded_args: Vec<vir::Expr>,
    /// The function return value, encoded as a Viper expression.
    encoded_return: vir::Expr,
    /// The labels at the end and the beginning of the function or before and after the function
    /// call.
    pre_label: &'a str,
    post_label: &'a str
}

impl<'a, 'p, 'v: 'p, 'tcx: 'v> ExpirationToolEncoder<'a, 'p, 'v, 'tcx> {
    fn new(
        procedure_encoder: &'a mut ProcedureEncoder<'p, 'v, 'tcx>,
        contract: &'a ProcedureContract<'tcx>,
        return_location: Option<mir::Location>,
        call_location: Option<mir::Location>,
        pre_label: &'a str,
        post_label: &'a str
    ) -> Self {
        // The arguments as Viper expressions.
        let encoded_args = contract.args.iter()
            .map(|local| procedure_encoder.encode_prusti_local(*local).into())
            .collect();

        // The return value as a Viper expression.
        let encoded_return = procedure_encoder.encode_prusti_local(contract.returned_value).into();

        ExpirationToolEncoder {
            procedure_encoder,
            contract,
            call_location, return_location,
            encoded_args, encoded_return,
            pre_label, post_label
        }
    }

    fn tcx(&self) -> ty::TyCtxt<'tcx> {
        self.procedure_encoder.procedure.get_tcx()
    }

    fn def_id(&self) -> DefId {
        self.contract.def_id
    }

    fn local_def_id(&self) -> LocalDefId {
        self.def_id().expect_local()
    }

    /// The MIR of the function that this expiration tool belongs to.
    /// TODO: The name `called_mir` may be a bit bad.
    fn called_mir(&self) -> Ref<mir::Body<'tcx>> {
        let local_def_id = self.local_def_id();
        let (mir, _) = self.tcx().mir_promoted(ty::WithOptConstParam::unknown(local_def_id));
        mir.borrow()
    }
}
