use std::collections::HashSet;

use rustc_span::Span;

use crate::encoder::places;
use crate::encoder::places::Place;

use super::Context;

mod analyze;
mod errors;

pub(super) use analyze::identify_dependencies;

pub(super) type PledgeDependencies<'tcx> = HashSet<PledgeDependency<'tcx>>;

/// Pledges have dependencies in the sense that some inputs must have been unblocked and some
/// outputs must have expired by the time the pledge is included in a magic wand. This struct
/// captures a single such dependency.
///
/// This is best explained by example. Assume a pledge contains a subexpression
/// ```ignore
/// after_unblocked(p.x) == before_expiry(*result.0)
/// ```
/// This would generate the following `PledgeDependency` for the `after_unblocked` subexpression:
/// ```ignore
/// PledgeDependency {
///   context: AfterUnblocked,
///   place: *p
/// }
/// ```
/// And the following `PledgeDependency` for the `before_expiry` subexpression:
/// ```ignore
/// PledgeDependency {
///   context: BeforeExpiry,
///   place: *result.0
/// }
/// ```
#[derive(Debug, Clone, Hash)]
pub(super) struct PledgeDependency<'tcx> {
    pub(super) context: Context,
    pub(super) place: places::Place<'tcx>,
    span: Span
}

// The point of the manual implementation is to ignore the `span` field. This could be replaced by
// the [derivative](https://crates.io/crates/derivative) crate.
impl PartialEq for PledgeDependency<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.context == other.context && self.place == other.place
    }
}

impl Eq for PledgeDependency<'_> {}

impl<'tcx> PledgeDependency<'tcx> {
    /// Returns whether this dependency is satisfied.
    pub(super) fn is_satisfied(&self,
        blocking: &HashSet<Place<'tcx>>,
        blocked: &HashSet<Place<'tcx>>,
    ) -> bool {
        let pending = match self.context {
            Context::BeforeExpiry => blocking,
            Context::AfterUnblocked => blocked
        };
        !pending.contains(&self.place)
    }
}

pub(super) trait PledgeDependenciesSatisfied<'tcx> {
    /// Figures out if the pledge dependencies are newly satisfied, i.e., they are satisfied now
    /// but weren't satisfied before the places from `expired` did expire.
    fn are_satisfied(self,
        blocking: &HashSet<places::Place<'tcx>>,
        blocked: &HashSet<places::Place<'tcx>>,
    ) -> bool;
}

impl<'a, 'tcx: 'a, DS> PledgeDependenciesSatisfied<'tcx> for DS
where DS: IntoIterator<Item=&'a PledgeDependency<'tcx>> {
    fn are_satisfied(self,
        blocking: &HashSet<Place<'tcx>>,
        blocked: &HashSet<Place<'tcx>>,
    ) -> bool {
        self.into_iter().all(|pd| pd.is_satisfied(blocking, blocked))
    }
}
