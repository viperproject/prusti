use std::collections::HashMap;
use rustc_middle::{mir, ty, ty::TyCtxt};
use rustc_mir::borrow_check::universal_regions::UniversalRegions;
use rustc_mir::borrow_check::facts::AllFacts;
use rustc_mir::borrow_check::nll::PoloniusOutput;
use rustc_mir::borrow_check::location::LocationTable;
use rustc_mir::borrow_check::location::RichLocation;

#[derive(PartialOrd, Ord, PartialEq, Eq)]
pub struct Lifetime {
    region: ty::RegionVid,
}

impl std::fmt::Debug for Lifetime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "'{}", self.region.index())
    }
}

impl From<ty::RegionVid> for Lifetime {
    fn from(region: ty::RegionVid) -> Self {
        Self { region }
    }
}

/// A constraint between two lifetimes: the `longer` lifetime outlives the
/// `shorter` lifetime.
///
/// Note: We cannot have “equal” constraint because the compiler gives us only
/// “outlives”. (Equal is represented as symmetric outlives constraints.)
#[derive(Debug, PartialOrd, Ord, PartialEq, Eq)]
pub struct LifetimeConstraint {
    /// The lifetime that outlives the `shorter` lifetime.
    longer: Lifetime,
    /// The lifetime that is outlived by the `longer` lifetime.
    shorter: Lifetime,
}

impl std::fmt::Display for LifetimeConstraint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?} ⊒ {:?}", self.longer, self.shorter)
    }
}

/// The point in the execution of the MIR statement.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Point {
    /// `Start` corresponds to `rustc_mir::borrow_check::location::RichLocation::Start`.
    Start,
    /// `Middle` corresponds to `rustc_mir::borrow_check::location::RichLocation::Mid`.
    Middle,
}

impl From<RichLocation> for Point {
    fn from(rich_location: RichLocation) -> Self {
        match rich_location {
            RichLocation::Start(_) => Point::Start,
            RichLocation::Mid(_) => Point::Middle,
        }

    }
}

/// A location in the MIR body.
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(super) enum Location {
    /// A statement or terminator inside the basic block. Terminator is the last
    /// “statement” of the block. We do not have a separate variant for the
    /// terminator because
    Statement {
        basic_block: mir::BasicBlock,
        statement: usize,
        point: Point,
    },
    /// An edge between two basic blocks.
    Edge {
        start: mir::BasicBlock,
        end: mir::BasicBlock,
    },
}

impl From<RichLocation> for Location {
    fn from(rich_location: RichLocation) -> Self {
        let point: Point = rich_location.into();
        let mir_location = match rich_location {
            RichLocation::Start(location) | RichLocation::Mid(location) => location,
        };
        Self::Statement {
            basic_block: mir_location.block,
            statement: mir_location.statement_index,
            point
        }
    }
}

pub(super) struct BodyLifetimes {
    /// Lifetimes that must be alive for the entire function.
    pub(super) universal_lifetimes: Vec<Lifetime>,
    /// Constraints between the universal lifetimes.
    pub(super) universal_lifetime_constraints: Vec<LifetimeConstraint>,
    pub(super) lifetimes_live_on_entry_at: HashMap<Location, Vec<Lifetime>>,
    /// The lifetimes that start at the given program point.
    pub(super) lifetimes_starting_at: HashMap<Location, Vec<Lifetime>>,
    /// The lifetimes that end at the given program point.
    pub(super) lifetimes_ending_at: HashMap<Location, Vec<Lifetime>>,
    /// The lifetimes constraints generated at the given program point.
    pub(super) new_lifetime_constraints_at: HashMap<Location, Vec<LifetimeConstraint>>,
}

pub(super) fn compute_lifetimes<'tcx>(
    polonius_input_facts: &AllFacts,
    polonius_output_facts: &PoloniusOutput,
    location_table: &LocationTable,
) -> BodyLifetimes {
    let universal_lifetimes = polonius_input_facts.universal_region.iter().map(
        |&region| region.into()
    ).collect();
    let mut universal_lifetime_constraints: Vec<_> = polonius_input_facts.known_subset.iter().map(
        |&(longer, shorter)| {
            LifetimeConstraint { longer: longer.into(), shorter: shorter.into() }
        }
    ).collect();
    universal_lifetime_constraints.sort();

    let lifetimes_live_on_entry_at = polonius_output_facts.origin_live_on_entry.iter().map(|(&point, origins)| {
        let mir_location = location_table.to_location(point);
        let location = mir_location.into();
        let lifetimes = origins.iter().map(|&origin| origin.into()).collect();
        (location, lifetimes)
    }).collect();


    let lifetimes_starting_at = HashMap::new();
    let lifetimes_ending_at = HashMap::new();
    let new_lifetime_constraints_at = HashMap::new();
    BodyLifetimes {
        universal_lifetimes,
        universal_lifetime_constraints,
        lifetimes_live_on_entry_at,
        lifetimes_starting_at,
        lifetimes_ending_at,
        new_lifetime_constraints_at,
    }
}

/// The state used in abstract interpretation to compute lifetimes.
struct LifetimeInferenceState {
    /// Polonius input facts.
    input_facts: AllFacts,
}

