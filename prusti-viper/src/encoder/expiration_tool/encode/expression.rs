use std::collections::HashSet;
use std::iter;
use std::iter::once;

use itertools::Itertools;

use prusti_common::vir;
use prusti_common::vir::borrows::Borrow;
use prusti_common::vir::ExprIterator;
use prusti_interface::environment::mir_utils::EraseAllRegions;
use prusti_interface::environment::mir_utils::PlaceAddProjection;
use rustc_hir::Mutability;
use rustc_middle::mir;

use crate::encoder::borrows::ProcedureContract;
use crate::encoder::procedure_encoder::ProcedureEncoder;

use super::binding::Binding;
use super::binding::encode_binding;
use super::ExpirationToolEncoder;
use super::LiftBindings;
use super::super::ExpirationTool;
use super::super::MagicWand;
use super::super::PartialExpirationTool;
use super::utils::extract_before_expiry_after_unblocked;

impl<'p, 'v: 'p, 'tcx: 'v> ProcedureEncoder<'p, 'v, 'tcx> {
    pub fn encode_expiration_tool_as_expression<'c>(&mut self,
        expiration_tool: &ExpirationTool<'c, 'tcx>,
        contract: &ProcedureContract<'tcx>,
        call_location: Option<mir::Location>,
        pre_label: &str,
        post_label: &str
    ) -> vir::Expr {
        let mut encoder = ExpirationToolEncoder::new(
            self, contract, None, call_location, pre_label, post_label);

        let (encoded_expiration_tool, bindings) =
            encoder.expiration_tool_as_expression(expiration_tool);

        // If there are still open bindings at this point we did something wrong.
        assert!(bindings.is_empty());

        encoded_expiration_tool
    }

    /// This encodes the given magic wand as a Viper expression. It also returns the bindings
    /// that are materialized as let-bindings in this magic wand (second tuple element) and the
    /// bindings that are still open (third tuple element).
    pub fn encode_magic_wand_as_expression<'c>(&mut self,
        magic_wand: &MagicWand<'c, 'tcx>,
        contract: &ProcedureContract<'tcx>,
        call_location: Option<mir::Location>,
        pre_label: &str,
        post_label: &str
    ) -> (vir::Expr, HashSet<Binding>, HashSet<Binding>) {
        let mut encoder = ExpirationToolEncoder::new(
            self, contract, None, call_location, pre_label, post_label);
        encoder.magic_wand_as_expression(magic_wand)
    }
}

impl<'c, 'a, 'p, 'v: 'p, 'tcx: 'v> ExpirationToolEncoder<'a, 'p, 'v, 'tcx> {
    /// This encodes the given expiration tool as a Viper expression.
    pub(super) fn expiration_tool_as_expression(&mut self,
        expiration_tool: &ExpirationTool<'c, 'tcx>
    ) -> (vir::Expr, HashSet<Binding>) {
        let (partial_expiration_tools, bindings) = expiration_tool.into_iter()
            .map(|partial_expiration_tool|
                self.partial_expiration_tool_as_expression(partial_expiration_tool))
            .lift_bindings();
        let expiration_tool = partial_expiration_tools.into_iter().conjoin();
        (expiration_tool, bindings)
    }

    pub(super) fn partial_expiration_tool_as_expression(&mut self,
        partial_expiration_tool: &PartialExpirationTool<'c, 'tcx>
    ) -> (vir::Expr, HashSet<Binding>) {
        let (branches, bindings) = self.encode_expiration_tool_branches(
            partial_expiration_tool,
            |encoder, antecedent, magic_wand| {
                let (encoded_magic_wand, _, bindings) =
                    encoder.magic_wand_as_expression(magic_wand);
                let encoded_branch = vir!([antecedent] ==> [encoded_magic_wand]);
                Ok((encoded_branch, bindings))
            }
        ).unwrap();
        let branches = branches.into_iter().conjoin();
        (branches, bindings)
    }

    /// This encodes the given magic wand as a Viper expression. It also returns the bindings
    /// that are materialized as let-bindings in this magic wand (second tuple element) and the
    /// bindings that are still open (third tuple element).
    pub(super) fn magic_wand_as_expression(&mut self,
        magic_wand: &MagicWand<'c, 'tcx>
    ) -> (vir::Expr, HashSet<Binding>, HashSet<Binding>) {
        let expired_borrow: Option<Borrow> = if self.call_location.is_some() {
            let expired_place = magic_wand.expired().to_mir_place().truncate(self.tcx(), 1);
            let expired_place = self.tcx().erase_all_regions(&expired_place);
            let polonius_info = self.procedure_encoder.polonius_info();
            let expired_loan = polonius_info.call_place_to_loan[&expired_place];
            Some(expired_loan.into())
        } else { None };

        let expired_perm = self.procedure_encoder.encode_place_perm(
            magic_wand.expired(), Mutability::Mut, self.call_location, self.post_label);

        let unblocked_perm = magic_wand.unblocked()
            .map(|unblocked| self.procedure_encoder.encode_place_perm(
                unblocked, Mutability::Mut, self.call_location, self.pre_label
            ))
            .conjoin();

        let nested_expiration_tool = self.expiration_tool_as_expression(magic_wand.expiration_tool);

        let pledges = magic_wand.pledges()
            .map(|(pledge, namespace)| {
                let pledge = self.encode_pledge(pledge);
                extract_before_expiry_after_unblocked(pledge, &mut namespace.clone())
            })
            .collect::<Vec<_>>();

        // A list of conjuncts representing the encoded pledges and expiration tools, with a list
        // of open bindings.
        let (pledges_and_nested_expiration_tool, bindings) = iter::empty()
            .chain(pledges)
            .chain(once(nested_expiration_tool))
            .lift_bindings();

        // A single expression encoding the pledges and nested expiration tools, still without let
        // bindings.
        let pledges_and_nested_expiration_tool =
            pledges_and_nested_expiration_tool.into_iter().conjoin();

        let (ripe_bindings, open_bindings) = self.partition_bindings(
            bindings, magic_wand.expired(), magic_wand.unblocked());

        // Wrap the pledges and nested expiration tools with the let bindings that can be evaluated
        // right now.
        let pledges_and_nested_expiration_tool = ripe_bindings.clone().into_iter().sorted()
            .fold(pledges_and_nested_expiration_tool, |rhs, binding| {
                let (var, expr) = encode_binding(binding, "lhs");
                let pos = rhs.pos();
                vir::Expr::LetExpr(var, Box::new(expr), Box::new(rhs), pos)
            });

        let expr = vir!([expired_perm] {expired_borrow} --* (
            [unblocked_perm] &&
            [pledges_and_nested_expiration_tool]
        ));

        (expr, ripe_bindings, open_bindings)
    }
}
