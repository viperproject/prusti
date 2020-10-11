use rustc_index::vec::Idx;
use rustc_middle::mir;
use rustc_middle::ty;

use super::TupleItemsForTy;
use super::PlaceAddProjection;

pub trait SplitAggregateAssignment<'tcx> {
    /// Transforms an assignment into its atomic parts. For a normal assignment like
    /// ```ignore
    ///     _3 = move _2;
    /// ```
    /// this is simply returning the assignment itself. But for an aggregate assignment like
    /// ```ignore
    ///     _3 = (move _1, move _2);
    /// ```
    /// there is one atomic assignment for every tuple field. In this example, the atomic
    /// assignments would be
    /// ```ignore
    ///     _3.0 = move _1;
    ///     _3.1 = move _2;
    /// ```
    ///
    /// Statements that are no assignments are returned untouched.
    fn split_assignment(self,
        tcx: ty::TyCtxt<'tcx>,
        mir: &mir::Body<'tcx>
    ) -> Vec<mir::Statement<'tcx>>;
}

impl<'tcx> SplitAggregateAssignment<'tcx> for mir::Statement<'tcx> {
    fn split_assignment(self,
        tcx: ty::TyCtxt<'tcx>,
        mir: &mir::Body<'tcx>
    ) -> Vec<mir::Statement<'tcx>> {
        let (lhs, rhs) = match self.kind {
            mir::StatementKind::Assign(box (lhs, rhs)) => (lhs, rhs),
            _ => return vec![self]
        };

        let atomic_assignments = match rhs {
            mir::Rvalue::Aggregate(box kind, operands) => {
                assert_eq!(kind, mir::AggregateKind::Tuple,
                    "The only supported aggregates are tuples.");
                let local = lhs.as_local().unwrap();
                let items_ty = mir.local_decls[local].ty.tuple_items().unwrap();
                operands.into_iter().zip(items_ty.into_iter())
                    .enumerate()
                    .map(|(i, (rhs, ty))| {
                        let field = mir::Field::new(i);
                        let lhs = tcx.mk_place_field(local.into(), field, ty);
                        let rhs = mir::Rvalue::Use(rhs);
                        (lhs, rhs)
                    })
                    .collect()
            }
            mir::Rvalue::Use(_) |
            mir::Rvalue::Ref(_, _, _) =>
                vec![(lhs, rhs)],
            _ => unreachable!("Rvalue {:?} is not supported", rhs)
        };

        let source_info = self.source_info;
        atomic_assignments.into_iter()
            .map(|(lhs, rhs)| {
                let kind = mir::StatementKind::Assign(box (lhs, rhs));
                mir::Statement { source_info, kind }
            })
            .collect()
    }
}

