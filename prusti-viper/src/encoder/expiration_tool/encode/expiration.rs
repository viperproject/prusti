use prusti_common::vir;

use crate::encoder::procedure_encoder::ProcedureEncoder;

use super::binding::Binding;
use crate::encoder::expiration_tool::encode::binding::encode_binding;

impl<'p, 'v: 'p, 'tcx: 'v> ProcedureEncoder<'p, 'v, 'tcx> {
    pub fn encode_bindings_as_assignments(&self,
        bindings: Vec<Binding>,
        lhs_label: &str
    ) -> (Vec<vir::LocalVar>, Vec<vir::Stmt>) {
        bindings.into_iter().map(|binding|
            self.encode_binding_as_assignment(binding, lhs_label)
        ).unzip()
    }

    fn encode_binding_as_assignment(&self,
        binding: Binding,
        lhs_label: &str
    ) -> (vir::LocalVar, vir::Stmt) {
        let (var, expr) = encode_binding(binding, lhs_label);
        let assignment = vir::Stmt::Assign(
            vir::Expr::local(var.clone()), expr, vir::AssignKind::Ghost);
        (var, assignment)
    }
}
