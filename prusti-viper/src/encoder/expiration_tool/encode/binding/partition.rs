use std::collections::HashSet;

use crate::encoder::places;

use super::Binding;
use super::super::ExpirationToolEncoder;
use super::super::super::Context::*;
use super::super::utils::contains_subexpr;

impl<'a, 'p, 'u, 'v: 'p, 'tcx: 'u + 'v> ExpirationToolEncoder<'a, 'p, 'v, 'tcx> {
    /// Determine which bindings are "ripe" (and can be materialized into let expressions right
    /// now). A binding is ripe if the output it depends on just expired or the input it depends
    /// on was just unblocked. The ripe bindings will become let expressions in the next step,
    /// the other bindings will be passed on to the caller.
    pub(in super::super) fn partition_bindings(&mut self,
        bindings: impl IntoIterator<Item=Binding>,
        expired: &places::Place<'tcx>,
        unblocked: impl IntoIterator<Item=&'u places::Place<'tcx>>
    ) -> (HashSet<Binding>, HashSet<Binding>) {
        let procedure_encoder = &mut self.procedure_encoder;
        let call_location = self.call_location;

        let (encoded_expired, _, _) = procedure_encoder.encode_generic_place(
            procedure_encoder.procedure.get_id(), call_location, expired);

        let encoded_unblocked = unblocked.into_iter().map(|unblocked| {
            let (encoded_unblocked, _, _) = procedure_encoder.encode_generic_place(
                procedure_encoder.procedure.get_id(), call_location, unblocked);
            encoded_unblocked
        }).collect::<Vec<_>>();

        bindings.into_iter()
            .partition(|Binding(_, context, bound_expr)| match context {
                BeforeExpiry => contains_subexpr(bound_expr, &encoded_expired),
                AfterUnblocked => encoded_unblocked.iter().any(|encoded_unblocked|
                    contains_subexpr(bound_expr, &encoded_unblocked))
            })
    }
}
