use std::collections::HashSet;

use prusti_common::vir;
use prusti_interface::environment::borrowck::facts::Loan;
use prusti_interface::environment::borrowck::facts::PointType;
use prusti_interface::environment::mir_utils::PlaceAddProjection;
use prusti_interface::specs::typed::Spanned;
use rustc_hir::Mutability;
use rustc_middle::mir;

use crate::encoder::borrows::ProcedureContract;
use crate::encoder::errors::ErrorCtxt;
use crate::encoder::places;
use crate::encoder::procedure_encoder::ProcedureEncoder;
use crate::encoder::procedure_encoder::Result;

use super::binding::Binding;
use super::binding::encode_binding;
use super::binding::LiftBindings;
use super::ExpirationToolEncoder;
use super::super::ExpirationTool;
use super::super::MagicWand;
use super::super::PartialExpirationTool;

impl<'p, 'v: 'p, 'tcx: 'v> ProcedureEncoder<'p, 'v, 'tcx> {
    pub fn encode_expiration_tool_as_package<'c>(&mut self,
        expiration_tool: &ExpirationTool<'c, 'tcx>,
        contract: &ProcedureContract<'tcx>,
        location: mir::Location,
        pre_label: &str,
        post_label: &str
    ) -> Result<Vec<vir::Stmt>> {
        // All blocked permissions must be folded before the post label.
        let obtains = expiration_tool.into_iter()
            .flat_map(|partial_expiration_tool| &partial_expiration_tool.blocking)
            .flat_map(|place| {
                let place_perm = self.encode_place_perm(place, Mutability::Mut, None, post_label);
                let place_perm = place_perm.map_labels(|label|
                    if label == post_label { None } else { Some(label) });
                self.encode_obtain(place_perm, vir::Position::default())
            })
            .collect::<Vec<_>>();

        // The post label.
        let post_label_stmt = vec![vir::Stmt::Label(post_label.to_owned())];

        let mut encoder = ExpirationToolEncoder::new(
            self, contract, Some(location), None, pre_label, post_label);

        // The expiration tool proper.
        let (package_stmts, bindings) = encoder.expiration_tool_as_package(expiration_tool)?;

        // If there are still open bindings at this point we did something wrong.
        assert_eq!(bindings.len(), 0);

        // Everything concatenated.
        Ok([
            &obtains[..],
            &post_label_stmt[..],
            &package_stmts[..]
        ].concat())
    }
}

impl<'c, 'a, 'p, 'v: 'p, 'tcx: 'v> ExpirationToolEncoder<'a, 'p, 'v, 'tcx> {
    /// This encodes the given expiration tool as a sequence of Viper statements that package
    /// all the partial expiration tools it contains directly.
    pub(super) fn expiration_tool_as_package(&mut self,
        expiration_tool: &ExpirationTool<'c, 'tcx>
    ) -> Result<(Vec<vir::Stmt>, HashSet<Binding>)> {
        let (package_expiration_tool, bindings) =
            expiration_tool.partial_expiration_tools()
                .map(|et| self.partial_expiration_tool_as_package(et))
                .collect::<Result<Vec<_>>>()?.into_iter()
                .lift_bindings();
        let package_expiration_tool = package_expiration_tool
            .into_iter().flatten().collect::<Vec<_>>();
        Ok((package_expiration_tool, bindings))
    }

    /// This encodes the given partial expiration tool as a sequence of Viper statements that
    /// package all the magic wands it contains directly.
    pub(super) fn partial_expiration_tool_as_package(&mut self,
        partial_expiration_tool: &PartialExpirationTool<'c, 'tcx>
    ) -> Result<(Vec<vir::Stmt>, HashSet<Binding>)> {
        self.encode_expiration_tool_branches(
            partial_expiration_tool,
            |encoder, antecedent, magic_wand| {
                let (encoded_magic_wand, bindings) =
                    encoder.magic_wand_as_package(magic_wand)?;
                let branch = vir!(if ([antecedent]) { [encoded_magic_wand] });
                Ok((branch, bindings))
            }
        )
    }

    /// This encodes the given magic wand as a Viper package statement.
    fn magic_wand_as_package(&mut self,
        magic_wand: &MagicWand<'c, 'tcx>
    ) -> Result<(vir::Stmt, HashSet<Binding>)> {
        let (magic_wand_expr, _, magic_wand_bindings) = self.magic_wand_as_expression(magic_wand);
        let (lhs, rhs) = match magic_wand_expr {
            vir::Expr::MagicWand(lhs, rhs, _, _) => (lhs.as_ref().clone(), rhs.as_ref().clone()),
            _ => unreachable!()
        };

        // Determine the loans that are kept alive by the expired reference only.
        let (expired_loans, expired_zombie_loans) = self.loans_only_kept_alive_by(
            magic_wand.expired(), magic_wand.blocking());

        // Expire the loans that are kept alive by the expired reference.
        let expire_loans = self.procedure_encoder.encode_expiration_of_loans(
            expired_loans, &expired_zombie_loans, self.return_location.unwrap(), None)?;

        // TODO: Explain what this is doing.
        let transfer_permissions_to_old = magic_wand.unblocked()
            .flat_map(|unblocked| self.procedure_encoder.transfer_permissions_to_old(
                unblocked, self.return_location.unwrap(), &self.encoded_args, self.pre_label))
            .collect::<Vec<_>>();

        // Fold the places that are unblocked by this magic wand.
        let fold_unblocked_references = magic_wand.unblocked()
            .flat_map(|unblocked| self.procedure_encoder.fold_unblocked_reference(
                unblocked, &self.encoded_args, self.pre_label))
            .collect::<Vec<_>>();

        // Package the nested expiration tool.
        let (package_expiration_tool, expiration_tool_bindings) =
            self.expiration_tool_as_package(magic_wand.expiration_tool)?;

        // Determine which of the bindings required by the nested expiration tool are "ripe"
        // (and can be turned into variables right away) and which bindings need to be passed on
        // to the parent expiration tool.
        let (ripe_expiration_tool_bindings, expiration_tool_bindings) =
            self.partition_bindings(expiration_tool_bindings,
                magic_wand.expired(), magic_wand.unblocked());

        let (vars, materialize_ripe_expiration_tool_bindings): (Vec<_>, Vec<_>) =
            ripe_expiration_tool_bindings.into_iter().map(|binding| {
                let (var, expr) = encode_binding(binding, "lhs");
                let assignment = vir::Stmt::Assign(
                    vir::Expr::local(var.clone()), expr, vir::AssignKind::Copy);
                (var, assignment)
            }).unzip();

        // The open bindings are the ones that are required by this magic wand itself or by any
        // of the nested expiration tools.
        let open_bindings = std::iter::empty()
            .chain(magic_wand_bindings)
            .chain(expiration_tool_bindings)
            .collect::<HashSet<_>>();

        let package_body_stmts: Vec<_> = [
            &expire_loans[..],
            &transfer_permissions_to_old[..],
            &materialize_ripe_expiration_tool_bindings[..],
            &fold_unblocked_references[..],
            &package_expiration_tool[..]
        ].concat();

        let spans = magic_wand.pledges()
            .flat_map(|(pledge, _)| pledge.get_spans(&self.called_mir(), self.tcx()))
            .collect::<Vec<_>>();
        let position = self.procedure_encoder.encoder.error_manager()
            .register(spans, ErrorCtxt::PackageMagicWandForPostcondition);

        let package_stmt = vir::Stmt::package_magic_wand(
            lhs, rhs, package_body_stmts,
            self.post_label.to_owned(),
            vars,
            position);

        Ok((package_stmt, open_bindings))
    }

    fn loans_only_kept_alive_by(&self,
        expired: &places::Place<'tcx>,
        blocking: impl Iterator<Item=&'a places::Place<'tcx>>
    ) -> (Vec<Loan>, Vec<Loan>) {
        let (maybe_expired_loans, maybe_expired_zombie_loans) = self.loans_kept_alive_by(expired);
        let maybe_expired_loans = maybe_expired_loans
            .into_iter().collect::<HashSet<_>>();
        let maybe_expired_zombie_loans = maybe_expired_zombie_loans
            .into_iter().collect::<HashSet<_>>();
        let (live_loans, live_zombie_loans): (Vec<_>, Vec<_>) = blocking
            .map(|place| self.loans_kept_alive_by(place))
            .unzip();
        let live_loans = live_loans.into_iter().flatten().collect::<HashSet<_>>();
        let live_zombie_loans = live_zombie_loans.into_iter().flatten().collect::<HashSet<_>>();
        let expired_loans = maybe_expired_loans
            .difference(&live_loans).cloned().collect();
        let expired_zombie_loans = maybe_expired_zombie_loans
            .difference(&live_zombie_loans).cloned().collect();
        (expired_loans, expired_zombie_loans)
    }

    fn loans_kept_alive_by(&self, place: &places::Place<'tcx>) -> (Vec<Loan>, Vec<Loan>) {
        let place = place.as_normal_place().unwrap();
        let base = place.truncate(self.procedure_encoder.procedure.get_tcx(), 1);
        let region = self.procedure_encoder.polonius_info().place_regions.for_place(&base).unwrap();
        let point_index = self.procedure_encoder.polonius_info().get_point(
            self.return_location.unwrap(), PointType::Start);
        self.procedure_encoder.polonius_info().get_all_loans_kept_alive_by(point_index, region)
    }
}

impl<'p, 'v: 'p, 'tcx: 'v> ProcedureEncoder<'p, 'v, 'tcx> {
    // TODO: What purpose serves this?
    fn transfer_permissions_to_old(&mut self,
        unblocked: &places::Place<'tcx>,
        location: mir::Location,
        encoded_args: &[vir::Expr],
        pre_label: &str
    ) -> Vec<vir::Stmt> {
        let (unblocked, _, _) = self.encode_generic_place(
            self.procedure.get_id(), None, unblocked);
        let contract = self.procedure_contract.as_ref().unwrap();
        let old_unblocked = self.wrap_arguments_into_old(
            unblocked.clone(), pre_label, contract, encoded_args);

        self.encode_transfer_permissions(unblocked, old_unblocked.clone(), location)
    }

    fn fold_unblocked_reference(&mut self,
        unblocked: &places::Place<'tcx>,
        encoded_args: &[vir::Expr],
        pre_label: &str
    ) -> Vec<vir::Stmt> {
        let (unblocked, _, _) = self.encode_generic_place(
            self.procedure.get_id(), None, unblocked);
        let contract = self.procedure_contract.as_ref().unwrap();
        let old_unblocked = self.wrap_arguments_into_old(
            unblocked.clone(), pre_label, contract, encoded_args);

        // Obtain the unblocked permission.
        let unblocked = vir::Expr::pred_permission(
            old_unblocked, vir::PermAmount::Write
        ).unwrap();
        self.encode_obtain(unblocked, vir::Position::default())
    }
}
