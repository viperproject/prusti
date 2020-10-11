use prusti_common::vir;
use rustc_hir::Mutability;
use rustc_middle::mir;

use crate::encoder::places;
use crate::encoder::procedure_encoder::ProcedureEncoder;

impl<'p, 'v: 'p, 'tcx: 'v> ProcedureEncoder<'p, 'v, 'tcx> {
    /// Encodes permissions for the given place as a Viper expression. The `location` must be
    /// `None` if the place is a normal place, and it must point to the function application if
    /// the place is a substituted place.
    pub fn encode_place_perm(&self,
        place: &places::Place<'tcx>,
        mutability: Mutability,
        location: Option<mir::Location>,
        label: &str
    ) -> vir::Expr {
        let perm_amount = match mutability {
            Mutability::Not => vir::PermAmount::Read,
            Mutability::Mut => vir::PermAmount::Write,
        };
        let (place_expr, place_ty, _) = self.encode_generic_place(
            self.procedure.get_id(), location, place);
        let vir_access = vir::Expr::pred_permission(place_expr.clone().old(label), perm_amount)
            .unwrap();
        let inv = self.encoder.encode_invariant_func_app(
            place_ty, place_expr.old(label));
        vir!([vir_access] && [inv])
    }
}
