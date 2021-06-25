// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fmt;
use std::path::PathBuf;

use datafrog;
use log::debug;
use log::trace;
use polonius_engine::Algorithm;
use polonius_engine::Atom;
use polonius_engine::Output;
// use rustc_hash::FxHashMap;
use rustc_data_structures::fx::FxHashMap;
use rustc_index::vec::Idx;
use rustc_middle::mir;
use rustc_middle::ty;
use rustc_span::def_id::LOCAL_CRATE;
use rustc_span::Span;
use rustc_index::vec::IndexVec;
// use rustc_mir::consumers::LocationTable;

use crate::environment::borrowck::facts::PointType;
use crate::environment::borrowck::regions::{PlaceRegions, PlaceRegionsError};
use crate::environment::mir_utils::AllPlaces;
use crate::environment::mir_utils::SplitAggregateAssignment;
use crate::environment::mir_utils::StatementAsAssign;
use crate::environment::mir_utils::StatementAt;
use crate::environment::polonius_info::facts::AllInputFacts;
use crate::utils;

use super::borrowck::facts;
use super::borrowck::regions;
use super::loops;
use super::mir_analyses::initialization::compute_definitely_initialized;
use super::mir_analyses::initialization::DefinitelyInitializedAnalysisResult;
use super::mir_analyses::liveness::compute_liveness;
use super::mir_analyses::liveness::LivenessAnalysisResult;
use super::procedure::Procedure;
use prusti_common::config;
use crate::environment::mir_utils::RealEdges;

/// This represents the assignment in which a loan was created. The `source`
/// will contain the creation of the loan, while the `dest` will store the
/// created reference.
#[derive(Clone, Debug)]
pub struct LoanPlaces<'tcx> {
    pub dest: mir::Place<'tcx>,
    pub source: mir::Rvalue<'tcx>,
    pub location: mir::Location,
}

/// We are guaranteed to have only the permissions that are currently
/// borrowed by the variable. For instance, in the example below `x`
/// borrows one variable while the other one is consumed. As a result,
/// we can restore the permissions only to the variable which we know
/// was borrowed by `x`.
/// ```rust,ignore
/// #![feature(nll)]
///
/// struct F { f: u32 }
///
/// fn consume_F(x: F) {}
///
/// fn test7(y: F, z: F, b: bool) {
///     let mut y = y;
///     let mut z = z;
///     let mut x;
///     if b {
///         x = &mut y;
///     } else {
///         x = &mut z;
///     }
///     let f = &mut x.f;
///     consume_F(y);
///     consume_F(z);
/// }
/// ```
///
/// Therefore, the loop magic wand should be:
/// ```silver
/// T(c) --* T(_orig_c)
/// ```
/// where `c` is a reference typed variable that is reassigned in the
/// loop and `_orig_c` is a ghost variable that stores the value `c` had
/// just before entering the loop. The package statement just before the
/// loop should be something like this:
/// ```silver
/// _orig_c := c.ref_val;
/// fold T$Ref(c);
/// package T$Ref(c) --* T(_orig_c) {
///     unfold T$Ref(c)
/// }
/// ```
#[derive(Clone)]
pub struct LoopMagicWand {
    /// Basic block that is the loop head.
    pub loop_id: mir::BasicBlock,
    /// The reference on the left hand side of the magic wand.
    pub variable: mir::Local,
    /// The region of the reference.
    pub region: facts::Region,
    /// The loan that is the root of the reborrowing DAG in the loop body.
    pub root_loan: facts::Loan,
}

impl fmt::Debug for LoopMagicWand {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "({:?}:{:?} --* _orig_{:?}_{:?})",
            self.variable, self.region, self.variable, self.loop_id
        )
    }
}

#[derive(Debug)]
pub enum ReborrowingKind {
    Assignment {
        /// The actual loan that expired.
        loan: facts::Loan,
    },
    /// Corresponds to the case when the reference was moved into function call.
    ArgumentMove {
        /// The actual loan that expired.
        loan: facts::Loan,
    },
    Call {
        /// The actual loan that expired.
        loan: facts::Loan,
        /// MIR local variable used as a target to which the result was assigned.
        variable: mir::Local,
        /// The region of the MIR local variable.
        region: facts::Region,
    },
    Loop {
        magic_wand: LoopMagicWand,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub enum ReborrowingZombity {
    /// The loan is not a zombie, which means that the borrowed path is
    /// still real.
    Real,
    /// The loan is a zombie, which means that the ghost variable
    /// created at given location should be used as the borrowed path.
    Zombie(mir::Location),
}

pub struct ReborrowingDAGNode {
    /// The loan to be restored.
    pub loan: facts::Loan,
    /// How this loan should be restored: by fold-unfold algorithm, by
    /// applying call magic wand, or by applying the loop magic wand.
    pub kind: ReborrowingKind,
    /// Is the loan a zombie?
    pub zombity: ReborrowingZombity,
    /// Are the loans reborrowing this one zombies?
    pub incoming_zombies: bool,
    /// Loans that are directly reborrowing this loan.
    pub reborrowing_loans: Vec<facts::Loan>,
    /// Loans that are directly reborrowed by this loan.
    pub reborrowed_loans: Vec<facts::Loan>,
}

impl fmt::Debug for ReborrowingDAGNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "(")?;
        match self.kind {
            ReborrowingKind::Assignment { .. } => {
                write!(f, "{:?}", self.loan)?;
            }
            ReborrowingKind::ArgumentMove { ref loan } => {
                write!(f, "Move({:?})", loan)?;
            }
            ReborrowingKind::Call {
                ref variable,
                ref region,
                ..
            } => {
                write!(f, "Call({:?}, {:?}:{:?})", self.loan, variable, region)?;
            }
            ReborrowingKind::Loop { ref magic_wand } => {
                write!(
                    f,
                    "Loop({:?}, {:?}:{:?}@{:?})",
                    magic_wand.root_loan,
                    magic_wand.variable,
                    magic_wand.region,
                    magic_wand.loop_id
                )?;
            }
        }
        match self.zombity {
            ReborrowingZombity::Real => {}
            ReborrowingZombity::Zombie(_) => {
                write!(f, ",zombie")?;
            }
        }
        if self.incoming_zombies {
            write!(f, ",in_zombie")?;
        }
        if !self.reborrowing_loans.is_empty() {
            write!(f, ",incoming=(")?;
            for loan in &self.reborrowing_loans {
                write!(f, "{:?},", loan)?;
            }
            write!(f, ")")?;
        }
        if !self.reborrowed_loans.is_empty() {
            write!(f, ",outgoing=(")?;
            for loan in &self.reborrowed_loans {
                write!(f, "{:?},", loan)?;
            }
            write!(f, ")")?;
        }
        write!(f, ")")?;
        Ok(())
    }
}

pub struct ReborrowingDAG {
    /// Loans sorted in topological order.
    nodes: Vec<ReborrowingDAGNode>,
}

impl ToString for ReborrowingDAG {
    fn to_string(&self) -> String {
        let nodes: Vec<_> = self
            .nodes
            .iter()
            .map(|node| format!("{:?}", node))
            .collect();
        nodes.join(";")
    }
}

impl ReborrowingDAG {
    pub fn iter(&self) -> impl Iterator<Item = &ReborrowingDAGNode> {
        self.nodes.iter()
    }

    pub fn get_node(&self, loan: facts::Loan) -> &ReborrowingDAGNode {
        let index = self
            .nodes
            .iter()
            .position(|node| node.loan == loan)
            .unwrap();
        &self.nodes[index]
    }
}

#[derive(Clone, Debug)]
pub enum PoloniusInfoError {
    /// Loans depending on branches inside loops are not implemented yet
    UnsupportedLoanInLoop {
        loop_head: mir::BasicBlock,
        variable: mir::Local,
    },
    LoansInNestedLoops(mir::Location, mir::BasicBlock, mir::Location, mir::BasicBlock),
    ReborrowingDagHasNoMagicWands(mir::Location),
    /// We currently support only one reborrowing chain per loop
    MultipleMagicWandsPerLoop(mir::Location),
    MagicWandHasNoRepresentativeLoan(mir::Location),
    PlaceRegionsError(PlaceRegionsError, Span),
    LoanInUnsupportedStatement(String, mir::Location),
}

// pub fn graphviz<'tcx>(
//     tcx: rustc_middle::ty::TyCtxt<'tcx>,
//     def_path: &rustc_hir::definitions::DefPath,
//     _mir: &mir::Body<'tcx>,
// ) -> std::io::Result<()> {
//     macro_rules! to_html {
//         ( $o:expr ) => {{
//             format!("{:?}", $o)
//                 .replace("{", "\\{")
//                 .replace("}", "\\}")
//                 .replace("&", "&amp;")
//                 .replace(">", "&gt;")
//                 .replace("<", "&lt;")
//                 .replace("\n", "<br/>")
//         }};
//     }
//     macro_rules! to_sorted_string {
//         ( $o:expr ) => {{
//             let mut vector = $o.iter().map(|x| to_html!(x)).collect::<Vec<String>>();
//             vector.sort();
//             vector.join(", ")
//         }};
//     }

//     let facts_loader = load_polonius_facts(tcx, def_path);
//     let interner = facts_loader.interner;
//     let borrowck_in_facts = facts_loader.facts;
//     let borrowck_out_facts = Output::compute(&borrowck_in_facts, Algorithm::Naive, true);

//     use std::io::Write;
//     let graph_path = PathBuf::from(config::log_dir())
//             .join("nll-facts")
//             .join(def_path.to_filename_friendly_no_crate())
//             .join("polonius.dot");
//     let graph_file = std::fs::File::create(graph_path).expect("Unable to create file");
//     let mut graph = std::io::BufWriter::new(graph_file);

//     let mut blocks: HashMap<_, _> = HashMap::new();
//     let mut block_edges = HashSet::new();
//     for (from_index, to_index) in borrowck_in_facts.cfg_edge {
//         let from = interner.get_point(from_index);
//         let from_block = from.location.block;
//         let to = interner.get_point(to_index);
//         let to_block = to.location.block;
//         let from_points = blocks.entry(from_block).or_insert(HashSet::new());
//         from_points.insert(from_index);
//         let to_points = blocks.entry(to_block).or_insert(HashSet::new());
//         to_points.insert(to_index);
//         if from_block != to_block {
//             block_edges.insert((from_block, to_block));
//         }
//     }

//     write!(graph, "digraph G {{\n")?;
//     write!(graph, "general [ shape=\"record\" ")?;
//     write!(graph, "label =<<table>\n")?;
//     write!(
//         graph,
//         "<tr><td>universal region:</td><td>{}</td></tr>\n",
//         to_sorted_string!(borrowck_in_facts.universal_region)
//     )?;
//     write!(
//         graph,
//         "<tr><td>placeholder:</td><td>{}</td></tr>\n",
//         to_sorted_string!(borrowck_in_facts.placeholder)
//     )?;
//     write!(graph, "</table>>];\n\n")?;
//     for (block, point_indices) in blocks {
//         write!(graph, "node_{:?} [ shape=\"record\" ", block)?;
//         write!(graph, "label =<<table>")?;
//         write!(graph, "<th><td>{:?}</td></th>\n", block)?;
//         write!(graph, "<tr>")?;
//         write!(graph, "<td>point</td>")?;
//         write!(graph, "<td>borrow_live_at</td>")?;
//         write!(graph, "</tr>\n")?;
//         let mut points: Vec<_> = point_indices.iter().map(|index| interner.get_point(*index)).collect();
//         points.sort();
//         for point in points {
//             write!(graph, "<tr>\n")?;
//             write!(graph, "<td>{}</td>\n", point)?;
//             write!(graph, "<td>")?;
//             let point_index = interner.get_point_index(&point);
//             for loan in &borrowck_out_facts.borrow_live_at[&point_index] {
//                 write!(graph, "{:?},", loan)?;
//             }
//             write!(graph, "</td>")?;
//             write!(graph, "</tr>\n")?;
//         }
//         write!(graph, "</table>>];\n\n")?;
//     }
//     for (from, to) in block_edges {
//         write!(graph, "node_{:?} -> node_{:?};\n", from, to)?;
//     }
//     write!(graph, "}}\n")?;
//     Ok(())
// }

pub use rustc_mir::consumers::{compute_polonius_facts, BodyWithFacts};

fn load_polonius_facts<'tcx>(
    tcx: rustc_middle::ty::TyCtxt<'tcx>,
    body: &mir::Body<'tcx>,
    // def_path: &rustc_hir::definitions::DefPath,
) -> BodyWithFacts<'tcx> {
    // let dir_path = PathBuf::from(config::log_dir())
    //     .join("nll-facts")
    //     .join(def_path.to_filename_friendly_no_crate());
    // debug!("Reading facts from: {:?}", dir_path);
    // let mut facts_loader = facts::FactLoader::new();
    // facts_loader.load_all_facts(&dir_path);
    // facts_loader
    let promoted = IndexVec::new();   // FIXME: This is certainly wrong.
    compute_polonius_facts(tcx, body, promoted)
}

pub struct PoloniusInfo<'a, 'tcx: 'a> {
    pub(crate) tcx: ty::TyCtxt<'tcx>,
    pub(crate) mir: &'a mir::Body<'tcx>,
    pub(crate) borrowck_in_facts: facts::AllInputFacts,
    pub(crate) borrowck_out_facts: facts::AllOutputFacts,
    pub(crate) interner: facts::Interner,
    /// Position at which a specific loan was created.
    pub(crate) loan_position: HashMap<facts::Loan, mir::Location>,
    pub(crate) loan_at_position: HashMap<mir::Location, facts::Loan>,
    pub(crate) call_loan_at_position: HashMap<mir::Location, facts::Loan>,
    pub(crate) call_magic_wands: HashMap<facts::Loan, mir::Local>,
    pub place_regions: PlaceRegions,
    pub(crate) additional_facts: AdditionalFacts,
    /// Loop head → Vector of magic wands in that loop.
    pub(crate) loop_magic_wands: HashMap<mir::BasicBlock, Vec<LoopMagicWand>>,
    /// Loans that are created inside loops. Loan → loop head.
    pub(crate) loops: loops::ProcedureLoops,
    /// Fake loans that were created due to variable moves.
    pub(crate) reference_moves: Vec<facts::Loan>,
    /// Fake loans that were created due to arguments moved into calls.
    pub(crate) argument_moves: Vec<facts::Loan>,
    /// Facts without back edges.
    pub(crate) additional_facts_no_back: AdditionalFacts,
    /// Two loans are conflicting if they borrow overlapping places and
    /// are alive at overlapping regions.
    pub(crate) loan_conflict_sets: HashMap<facts::Loan, HashSet<facts::Loan>>
}

/// This creates a new loan for each move of a borrow. Moves occur either due to assignments or
/// due to function calls. It returns three values, in this order:
/// - The list loans that were created due to borrows moved by an assignment.
/// - The list loans that were created due to borrows moved by a function call.
/// - A list of incompatability sets. Every incompatability set contains loans that cannot be
///   reborrows of each other.
fn add_fake_facts<'a, 'tcx: 'a>(
    all_facts: &mut facts::AllInputFacts,
    interner: &facts::Interner,
    tcx: ty::TyCtxt<'tcx>,
    mir: &'a mir::Body<'tcx>,
    place_regions: &PlaceRegions,
    call_magic_wands: &mut HashMap<facts::Loan, mir::Local>,
) -> Result<
    (Vec<facts::Loan>, Vec<facts::Loan>, Vec<Vec<facts::Loan>>),
    (PlaceRegionsError, mir::Location)
> {
    let mut reference_moves = Vec::new();
    let mut argument_moves = Vec::new();
    let mut incompatible_loans = Vec::new();

    let mut last_loan_id = Iterator::chain(
        all_facts.borrow_region.iter()
            .map(|(_, loan, _)| loan.index()),
        all_facts.placeholder.iter()
            .map(|(_, loan)| loan.index())
    ).max().unwrap_or(0);

    let mut new_loan = || {
        last_loan_id += 1;
        facts::Loan::from(last_loan_id)
    };

    // Create a map from points to (region1, region2) vectors.
    let universal_region = &all_facts.universal_region;
    let mut outlives_at_point = HashMap::new();
    for &(region1, region2, point) in all_facts.outlives.iter() {
        if !universal_region.contains(&region1) && !universal_region.contains(&region2) {
            let outlives = outlives_at_point.entry(point).or_insert(vec![]);
            outlives.push((region1, region2));
        }
    }

    // Create new borrow_region facts for points where is only one outlives
    // fact and there is not a borrow_region fact already.
    let borrow_region = &mut all_facts.borrow_region;
    for (point, regions) in outlives_at_point {
        if borrow_region
            .iter()
            .all(|(_, _, loan_point)| *loan_point != point)
        {
            let location = interner.get_point(point).location.clone();
            if is_call(&mir, location) {
                // Add a fake loan for the returned reference.
                let call_destination = get_call_destination(&mir, location);
                if let Some(place) = call_destination {
                    debug!("Adding for call destination:");
                    for &(region1, region2) in regions.iter() {
                        debug!("{:?} {:?} {:?}", location, region1, region2);
                    }
                    let local = place.local;
                    if place.projection.len() > 0 {
                        unimplemented!();
                    }
                    if let Some(var_region) = place_regions.for_local(local) {
                        let loan = new_loan();
                        debug!("var_region = {:?} loan = {:?}", var_region, loan);
                        borrow_region.push((var_region, loan, point));
                        call_magic_wands.insert(loan, local);
                    }
                }
                // Add a fake loan for each reference argument passed into the call.
                for arg in get_call_arguments(&mir, location) {
                    if let Some(var_region) = place_regions.for_local(arg) {
                        let loan = new_loan();
                        debug!("var_region = {:?} loan = {:?}", var_region, loan);
                        borrow_region.push((var_region, loan, point));
                        argument_moves.push(loan);
                    }
                }
            } else if is_assignment(&mir, location) {
                // Fake loans for assignments are created here. The LHS of the assignment must be
                // a reference-typed local variable or a tuple-typed local variable with references
                // inside.
                let statement = mir.statement_at(location).unwrap();
                let (lhs, _) = statement.as_assign().unwrap();
                let lhs_places =
                    if let Some(local) = lhs.as_local() {
                        local.all_places(tcx, mir)
                    } else {
                        // TODO: The LHS may still be a tuple, even if it's not a local variable.
                        vec![lhs.clone()]
                    };
                let mut lhs_regions = vec![];
                for place in lhs_places.into_iter() {
                    if let Some(region) = place_regions
                        .for_place(place)
                        .map_err(|err| (err, location))? {
                        lhs_regions.push(region);
                    }
                }
                let mut new_incompatible = Vec::new();
                for lhs_region in lhs_regions {
                    let loan = new_loan();
                    reference_moves.push(loan);
                    borrow_region.push((lhs_region, loan, point));
                    new_incompatible.push(loan);
                    debug!("Adding generic: _ {:?} {:?} {:?}",
                        lhs_region, location, loan);
                }
                incompatible_loans.push(new_incompatible);
            }
        }
    }
    Ok((reference_moves, argument_moves, incompatible_loans))
}

/// Remove back edges to make MIR uncyclic so that we can compute reborrowing dags at the end of
/// the loop body.
fn remove_back_edges(
    mut all_facts: facts::AllInputFacts,
    interner: &facts::Interner,
    back_edges: &HashSet<(mir::BasicBlock, mir::BasicBlock)>,
) -> facts::AllInputFacts {
    let cfg_edge = all_facts.cfg_edge;
    let cfg_edge = cfg_edge
        .into_iter()
        .filter(|(from, to)| {
            let from_block = interner.get_point(*from).location.block;
            let to_block = interner.get_point(*to).location.block;
            let remove = back_edges.contains(&(from_block, to_block));
            if remove {
                debug!("remove cfg_edge: {:?} → {:?}", from_block, to_block);
            }
            !remove
        })
        .collect();
    all_facts.cfg_edge = cfg_edge;
    all_facts
}

/// Returns the place that is borrowed by the assignment. We assume that
/// all shared references are created only via assignments and ignore
/// all other cases.
fn get_borrowed_places<'a, 'tcx: 'a>(
    mir: &'a mir::Body<'tcx>,
    loan_position: &HashMap<facts::Loan, mir::Location>,
    loan: facts::Loan,
) -> Result<Vec<&'a mir::Place<'tcx>>, PoloniusInfoError> {
    let location = if let Some(location) = loan_position.get(&loan) {
        location
    } else {
        // FIXME (Vytautas): This is likely to be wrong.
        debug!("Not found: {:?}", loan);
        return Ok(Vec::new());
    };
    let mir::BasicBlockData { ref statements, .. } = mir[location.block];
    if statements.len() == location.statement_index {
        Ok(Vec::new())
    } else {
        let statement = &statements[location.statement_index];
        match statement.kind {
            mir::StatementKind::Assign(box (ref _lhs, ref rhs)) => match rhs {
                &mir::Rvalue::Ref(_, _, ref place) |
                &mir::Rvalue::Discriminant(ref place) |
                &mir::Rvalue::Use(mir::Operand::Copy(ref place)) |
                &mir::Rvalue::Use(mir::Operand::Move(ref place)) => {
                    Ok(vec![place])
                },
                &mir::Rvalue::Use(mir::Operand::Constant(_)) => {
                    Ok(Vec::new())
                },
                &mir::Rvalue::Aggregate(_, ref operands) => {
                    Ok(operands
                        .iter()
                        .flat_map(|operand| {
                            match operand {
                                mir::Operand::Copy(ref place) |
                                mir::Operand::Move(ref place) => Some(place),
                                mir::Operand::Constant(_) => None,
                            }
                        })
                        .collect())
                }
                // slice creation involves an unsize pointer cast like [i32; 3] -> &[i32]
                &mir::Rvalue::Cast(mir::CastKind::Pointer(ty::adjustment::PointerCast::Unsize), ref operand, ref ty) if ty.is_slice() && !ty.is_unsafe_ptr() => {
                    trace!("slice: operand={:?}, ty={:?}", operand, ty);
                    Ok(match operand {
                        mir::Operand::Copy(ref place) |
                            mir::Operand::Move(ref place) => vec![place],
                            mir::Operand::Constant(_) => vec![],
                    })
                }

                &mir::Rvalue::Cast(..) => {
                    // all other loan-casts are unsupported
                    Err(PoloniusInfoError::LoanInUnsupportedStatement(
                        "cast statements that create loans are not supported".to_string(),
                        *location,
                    ))
                }
                x => unreachable!("{:?}", x),
            },
            ref x => unreachable!("{:?}", x),
        }
    }
}

fn compute_loan_conflict_sets(
    procedure: &Procedure,
    loan_position: &HashMap<facts::Loan, mir::Location>,
    borrowck_in_facts: &facts::AllInputFacts,
    borrowck_out_facts: &facts::AllOutputFacts,
) -> Result<HashMap<facts::Loan, HashSet<facts::Loan>>, PoloniusInfoError> {
    trace!("[enter] compute_loan_conflict_sets");

    let mut loan_conflict_sets = HashMap::new();

    let mir = procedure.get_mir();

    for &(_r, loan, _) in &borrowck_in_facts.borrow_region {
        loan_conflict_sets.insert(loan, HashSet::new());
    }

    for &(_r, loan_created, point) in &borrowck_in_facts.borrow_region {
        let location = loan_position[&loan_created];
        if !procedure.is_reachable_block(location.block) || procedure.is_spec_block(location.block)
        {
            continue;
        }
        for borrowed_place in get_borrowed_places(mir, loan_position, loan_created)? {
            if let Some(live_borrows) = borrowck_out_facts.borrow_live_at.get(&point) {
                for loan_alive in live_borrows {
                    if loan_created == *loan_alive {
                        continue;
                    }
                    for place in get_borrowed_places(mir, loan_position, *loan_alive)? {
                        if utils::is_prefix(borrowed_place, place)
                            || utils::is_prefix(place, borrowed_place)
                        {
                            loan_conflict_sets
                                .get_mut(&loan_created)
                                .unwrap()
                                .insert(*loan_alive);
                            loan_conflict_sets
                                .get_mut(loan_alive)
                                .unwrap()
                                .insert(loan_created);
                        }
                    }
                }
            }
        }
    }

    trace!(
        "[exit] compute_loan_conflict_sets = {:?}",
        loan_conflict_sets
    );

    Ok(loan_conflict_sets)
}

impl<'a, 'tcx: 'a> PoloniusInfo<'a, 'tcx> {
    pub fn new(
        procedure: &'a Procedure<'a, 'tcx>,
        _loop_invariant_block: &HashMap<mir::BasicBlock, mir::BasicBlock>,
    ) -> Result<Self, PoloniusInfoError> {
        let tcx = procedure.get_tcx();
        let def_id = procedure.get_id();
        let mir = procedure.get_mir();
        let def_path = tcx.hir().def_path(def_id.expect_local());

        // Read Polonius facts.
        let facts = load_polonius_facts(tcx, mir);

        // // Read relations between region IDs and local variables.
        // let renumber_path = PathBuf::from(config::log_dir())
        //     .join("mir")
        //     .join(format!(
        //         "{}.{}.-------.renumber.0.mir",
        //         tcx.crate_name(LOCAL_CRATE),
        //         def_path.to_filename_friendly_no_crate()
        //     ));
        // debug!("Renumber path: {:?}", renumber_path);
        let place_regions = regions::load_place_regions(&facts).unwrap();

        let mut call_magic_wands = HashMap::new();

        let mut all_facts = facts.input_facts;
        let interner = facts::Interner::new(facts.location_table);

        let real_edges = RealEdges::new(&mir);
        let loop_info = loops::ProcedureLoops::new(&mir, &real_edges);
        let (reference_moves, argument_moves, incompatible_loans) = add_fake_facts(
            &mut all_facts,
            &interner,
            tcx,
            &mir,
            &place_regions,
            &mut call_magic_wands
        ).map_err(|(err, loc)|
            PoloniusInfoError::PlaceRegionsError(err, mir.source_info(loc).span)
        )?;

        Self::disconnect_universal_regions(tcx, mir, &place_regions, &mut all_facts)
            .map_err(|(err, loc)| PoloniusInfoError::PlaceRegionsError(err, loc))?;

        let output = Output::compute(&all_facts, Algorithm::Naive, true);
        let all_facts_without_back_edges = remove_back_edges(
            all_facts.clone(),
            &interner,
            &loop_info.back_edges,
        );
        let output_without_back_edges =
            Output::compute(&all_facts_without_back_edges, Algorithm::Naive, true);

        let loan_position: HashMap<_, _> = all_facts
            .borrow_region
            .iter()
            .map(|&(_, loan, point_index)| {
                let point = interner.get_point(point_index);
                (loan, point.location)
            })
            .collect();
        let loan_at_position: HashMap<_, _> = all_facts
            .borrow_region
            .iter()
            .map(|&(_, loan, point_index)| {
                let point = interner.get_point(point_index);
                (point.location, loan)
            })
            .collect();
        let call_loan_at_position: HashMap<_, _> = all_facts
            .borrow_region
            .iter()
            .filter(|&(_, loan, _)| call_magic_wands.contains_key(loan))
            .map(|&(_, loan, point_index)| {
                let point = interner.get_point(point_index);
                (point.location, loan)
            })
            .collect();

        let additional_facts = AdditionalFacts::new(&all_facts, &output, &incompatible_loans);
        let additional_facts_without_back_edges =
            AdditionalFacts::new(
                &all_facts_without_back_edges,
                &output_without_back_edges,
                &incompatible_loans);
        // FIXME: Check whether the new info in Polonius could be used for computing initialization.
        let loan_conflict_sets =
            compute_loan_conflict_sets(procedure, &loan_position, &all_facts, &output)?;

        let info = Self {
            tcx: tcx,
            mir: mir,
            borrowck_in_facts: all_facts,
            borrowck_out_facts: output,
            interner: interner,
            loan_position: loan_position,
            loan_at_position: loan_at_position,
            call_loan_at_position: call_loan_at_position,
            call_magic_wands: call_magic_wands,
            place_regions: place_regions,
            additional_facts: additional_facts,
            loop_magic_wands: HashMap::new(),
            additional_facts_no_back: additional_facts_without_back_edges,
            loops: loop_info,
            reference_moves: reference_moves,
            argument_moves: argument_moves,
            loan_conflict_sets: loan_conflict_sets,
        };
        // info.compute_loop_magic_wands(loop_invariant_block)?; FIXME
        Ok(info)
    }

    fn disconnect_universal_regions(
        tcx: ty::TyCtxt<'tcx>,
        mir: &mir::Body<'tcx>,
        place_regions: &PlaceRegions,
        all_facts: &mut AllInputFacts
    ) -> Result<(), (PlaceRegionsError, Span)> {
        let mut return_regions = vec![];
        let return_span = mir.local_decls[mir::RETURN_PLACE].source_info.span;
        for place in mir::RETURN_PLACE.all_places(tcx, mir).into_iter() {
            if let Some(region) = place_regions.for_place(place)
                .map_err(|err| (err, return_span))? {
                return_regions.push(region);
            }
        }

        let input_regions = (1..=mir.arg_count)
            .map(|i| mir::Local::new(i))
            .filter_map(|l| place_regions.for_local(l))
            .collect::<Vec<_>>();

        // Disconnect return regions from universal regions.
        let universal_region = &all_facts.universal_region;
        let is_universal = |r: &facts::Region| universal_region.contains(r);
        let is_input_region = |r: &facts::Region| input_regions.contains(r);
        all_facts.outlives.retain(|(r1, r2, _)| (
            !is_universal(r1) || is_input_region(r2)
        ) && (
            !is_universal(r2)
        ));

        // Add return regions to universal regions.
        all_facts.universal_region.extend(return_regions);
        Ok(())
    }

    // fn compute_loop_magic_wands(
    //     &mut self,
    //     _loop_invariant_block: &HashMap<mir::BasicBlock, mir::BasicBlock>,
    // ) -> Result<(), PoloniusInfoError> {
    //     trace!("[enter] compute_loop_magic_wands");
    //     let loop_heads = self.loops.loop_heads.clone();
    //     for &loop_head in loop_heads.iter() {
    //         debug!("loop_head = {:?}", loop_head);
    //         // TODO: Check whether we should use mut_borrow_leaves instead of write_leaves.
    //         let definitely_initalised_paths = self.initialization.get_before_block(loop_head);
    //         let (write_leaves, _mut_borrow_leaves, _read_leaves) =
    //             self.loops.compute_read_and_write_leaves(
    //                 loop_head,
    //                 self.mir,
    //                 Some(&definitely_initalised_paths),
    //             );
    //         debug!("write_leaves = {:?}", write_leaves);
    //         let reborrows: Vec<(mir::Local, facts::Region)> = write_leaves
    //             .iter()
    //             .flat_map(|place| {
    //                 // Only locals – we do not support references in fields.
    //                 match place {
    //                     mir::Place::Local(local) => Some(local),
    //                     _ => None,
    //                 }
    //             })
    //             .flat_map(|local| {
    //                 // Only references (variables that have regions).
    //                 self.place_regions
    //                     .for_local(*local)
    //                     .map(|region| (*local, region))
    //             })
    //             // Note: With our restrictions these two checks are sufficient to ensure
    //             // that we have reborrowing. For example, we do not need to check that
    //             // at least one of the loans is coming from inside of the loop body.
    //             .collect();
    //         debug!("reborrows = {:?}", reborrows);
    //         for &(local, _) in reborrows.iter() {
    //             debug!("loop_head = {:?} reborrow={:?}", loop_head, local);
    //             self.add_loop_magic_wand(loop_head, local)?;
    //         }
    //     }
    //     trace!("[exit] compute_loop_magic_wands");
    //     Ok(())
    // }

    pub fn get_point(
        &self,
        location: mir::Location,
        point_type: facts::PointType,
    ) -> facts::PointIndex {
        let point = facts::Point {
            location: location,
            typ: point_type,
        };
        self.interner.get_point_index(&point)
    }

    pub fn get_all_loans_kept_alive_by(
        &self,
        point: facts::PointIndex,
        region: facts::Region,
    ) -> (Vec<facts::Loan>, Vec<facts::Loan>) {
        let mut loans =
            self.get_loans_kept_alive_by(point, region, &self.borrowck_out_facts.restricts);
        let zombie_loans =
            self.get_loans_kept_alive_by(point, region, &self.additional_facts.zombie_requires);
        loans.extend(zombie_loans.iter().cloned());
        (loans, zombie_loans)
    }

    /// Get loans that are kept alive by the given region.
    fn get_loans_kept_alive_by(
        &self,
        point: facts::PointIndex,
        region: facts::Region,
        restricts_map: &FxHashMap<
            facts::PointIndex,
            BTreeMap<facts::Region, BTreeSet<facts::Loan>>,
        >,
    ) -> Vec<facts::Loan> {
        restricts_map
            .get(&point)
            .as_ref()
            .and_then(|restricts| restricts.get(&region))
            .map(|loans| loans.iter().cloned().collect())
            .unwrap_or(Vec::new())
    }

    /// Get loans that dye at the given location.
    pub(crate) fn get_dying_loans(&self, location: mir::Location) -> Vec<facts::Loan> {
        self.get_loans_dying_at(location, false)
    }

    /// Get loans that dye at the given location.
    pub(crate) fn get_dying_zombie_loans(&self, location: mir::Location) -> Vec<facts::Loan> {
        self.get_loans_dying_at(location, true)
    }

    /// Get loans including the zombies ``(all_loans, zombie_loans)``.
    pub fn get_all_active_loans(
        &self,
        location: mir::Location,
    ) -> (Vec<facts::Loan>, Vec<facts::Loan>) {
        let mut loans = self.get_active_loans(location, false);
        let zombie_loans = self.get_active_loans(location, true);
        loans.extend(zombie_loans.iter().cloned());
        (loans, zombie_loans)
    }

    fn get_borrow_live_at(&self, zombie: bool) -> &FxHashMap<facts::PointIndex, Vec<facts::Loan>> {
        if zombie {
            &self.additional_facts.zombie_borrow_live_at
        } else {
            &self.borrowck_out_facts.borrow_live_at
        }
    }

    /// Get loans that are active (including those that are about to die) at the given location.
    pub fn get_active_loans(&self, location: mir::Location, zombie: bool) -> Vec<facts::Loan> {
        let borrow_live_at = self.get_borrow_live_at(zombie);
        let start_point = self.get_point(location, facts::PointType::Start);
        let mid_point = self.get_point(location, facts::PointType::Mid);

        let mut loans = if let Some(mid_loans) = borrow_live_at.get(&mid_point) {
            let mut mid_loans = mid_loans.clone();
            mid_loans.sort();
            let default_vec = vec![];
            let start_loans = borrow_live_at.get(&start_point).unwrap_or(&default_vec);
            let mut start_loans = start_loans.clone();
            start_loans.sort();
            debug!("start_loans = {:?}", start_loans);
            debug!("mid_loans = {:?}", mid_loans);
            // Loans are created in mid point, so mid_point may contain more loans than the start
            // point.
            assert!(start_loans.iter().all(|loan| mid_loans.contains(loan)));

            mid_loans
        } else {
            assert!(borrow_live_at.get(&start_point).is_none());
            vec![]
        };
        if !zombie {
            for (_, loan, point) in self.borrowck_in_facts.borrow_region.iter() {
                if point == &mid_point && !loans.contains(loan) {
                    loans.push(*loan);
                }
            }
        }
        loans
    }

    /// Get loans including the zombies ``(all_loans, zombie_loans)``.
    pub fn get_all_loans_dying_at(
        &self,
        location: mir::Location,
    ) -> (Vec<facts::Loan>, Vec<facts::Loan>) {
        let mut loans = self.get_loans_dying_at(location, false);
        let zombie_loans = self.get_loans_dying_at(location, true);
        loans.extend(zombie_loans.iter().cloned());
        (loans, zombie_loans)
    }

    /// Get loans including the zombies ``(all_loans, zombie_loans)``.
    pub fn get_all_loans_dying_between(
        &self,
        initial_loc: mir::Location,
        final_loc: mir::Location,
    ) -> (Vec<facts::Loan>, Vec<facts::Loan>) {
        trace!(
            "[enter] get_all_loans_dying_between initial_loc={:?} final_loc={:?}",
            initial_loc,
            final_loc
        );

        let mut loans = self.get_loans_dying_between(initial_loc, final_loc, false);
        let zombie_loans = self.get_loans_dying_between(initial_loc, final_loc, true);
        loans.extend(zombie_loans.iter().cloned());
        trace!(
            "[exit] get_all_loans_dying_between \
             initial_loc={:?} final_loc={:?} all={:?} zombie={:?}",
            initial_loc,
            final_loc,
            loans,
            zombie_loans
        );
        (loans, zombie_loans)
    }

    /// Get loans that die *at* (that is, exactly after) the given location.
    pub fn get_loans_dying_at(&self, location: mir::Location, zombie: bool) -> Vec<facts::Loan> {
        let borrow_live_at = self.get_borrow_live_at(zombie);
        let successors = self.get_successors(location);
        let is_return = is_return(self.mir, location);
        let mid_point = self.get_point(location, facts::PointType::Mid);
        let becoming_zombie_loans = self
            .additional_facts
            .borrow_become_zombie_at
            .get(&mid_point)
            .cloned()
            .unwrap_or(Vec::new());
        self.get_active_loans(location, zombie)
            .into_iter()
            .filter(|loan| {
                let alive_in_successor = successors.iter().any(|successor_location| {
                    let point = self.get_point(*successor_location, facts::PointType::Start);
                    borrow_live_at
                        .get(&point)
                        .map_or(false, |successor_loans| successor_loans.contains(loan))
                });
                !alive_in_successor && !(successors.is_empty() && is_return)
            })
            .filter(|loan| !becoming_zombie_loans.contains(loan))
            .collect()
    }

    /// Get loans that die between two consecutive locations
    pub fn get_loans_dying_between(
        &self,
        initial_loc: mir::Location,
        final_loc: mir::Location,
        zombie: bool,
    ) -> Vec<facts::Loan> {
        trace!(
            "[enter] get_loans_dying_between {:?}, {:?}, {}",
            initial_loc,
            final_loc,
            zombie
        );
        debug_assert!(self.get_successors(initial_loc).contains(&final_loc));
        let mid_point = self.get_point(initial_loc, facts::PointType::Mid);
        let becoming_zombie_loans = self
            .additional_facts
            .borrow_become_zombie_at
            .get(&mid_point)
            .cloned()
            .unwrap_or(Vec::new());
        trace!("becoming_zombie_loans={:?}", becoming_zombie_loans);
        let final_loc_point = self.get_point(final_loc, facts::PointType::Start);
        trace!(
            "borrow_live_at final {:?}",
            self.borrowck_out_facts.borrow_live_at.get(&final_loc_point)
        );
        let dying_loans = self
            .get_active_loans(initial_loc, zombie)
            .into_iter()
            .filter(|loan| {
                self.get_borrow_live_at(zombie)
                    .get(&final_loc_point)
                    .map_or(true, |successor_loans| !successor_loans.contains(loan))
            })
            .filter(|loan| !becoming_zombie_loans.contains(loan))
            .collect();
        trace!(
            "[exit] get_loans_dying_between {:?}, {:?}, {}, dying_loans={:?}",
            initial_loc,
            final_loc,
            zombie,
            dying_loans
        );
        dying_loans
    }

//     /// Get loans including the zombies ``(all_loans, zombie_loans)``.
//     pub fn get_all_loans_dying_before(
//         &self,
//         location: mir::Location,
//     ) -> (Vec<facts::Loan>, Vec<facts::Loan>) {
//         let mut loans = self.get_loans_dying_before(location, false);
//         let zombie_loans = self.get_loans_dying_before(location, true);
//         loans.extend(zombie_loans.iter().cloned());
//         (loans, zombie_loans)
//     }

//     /// Get loans that die exactly before the given location, but not *at* any of the predecessors.
//     /// Note: we don't handle a loan that dies just in a subset of the incoming CFG edges.
//     pub fn get_loans_dying_before(
//         &self,
//         location: mir::Location,
//         zombie: bool,
//     ) -> Vec<facts::Loan> {
//         let mut predecessors = self.get_predecessors(location);
//         let mut dying_before: Option<HashSet<facts::Loan>> = None;
//         for predecessor in predecessors.drain(..) {
//             let dying_at_predecessor: HashSet<_> =
//                 HashSet::from_iter(self.get_loans_dying_at(predecessor, zombie));
//             let dying_between: HashSet<_> =
//                 HashSet::from_iter(self.get_loans_dying_between(predecessor, location, zombie));
//             let dying_before_loc: HashSet<_> = dying_between
//                 .difference(&dying_at_predecessor)
//                 .cloned()
//                 .collect();
//             if let Some(ref dying_before_content) = dying_before {
//                 if dying_before_content != &dying_before_loc {
//                     debug!("Incoming CFG edges have different expiring loans");
//                     return vec![];
//                 }
//             } else {
//                 dying_before = Some(dying_before_loc);
//             }
//         }
//         dying_before
//             .map(|d| d.into_iter().collect())
//             .unwrap_or(vec![])
//     }

    pub fn get_conflicting_loans(&self, loan: facts::Loan) -> Vec<facts::Loan> {
        self.loan_conflict_sets
            .get(&loan)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or(Vec::new())
    }

    pub fn get_alive_conflicting_loans(
        &self,
        loan: facts::Loan,
        location: mir::Location,
    ) -> Vec<facts::Loan> {
        if let Some(all_conflicting_loans) = self.loan_conflict_sets.get(&loan) {
            let point = self.get_point(location, facts::PointType::Mid);
            if let Some(alive_loans) = self.borrowck_out_facts.borrow_live_at.get(&point) {
                let alive_conflicting_loans = all_conflicting_loans
                    .iter()
                    .filter(|loan| alive_loans.contains(loan))
                    .cloned()
                    .collect();
                trace!(
                    "get_alive_conflicting_loans({:?}, {:?}) = {:?}",
                    loan,
                    location,
                    alive_conflicting_loans
                );
                return alive_conflicting_loans;
            }
        }
        Vec::new()
    }

    pub fn get_loan_location(&self, loan: &facts::Loan) -> mir::Location {
        self.loan_position.get(loan).unwrap_or_else(
            || {panic!("not found: {:?}", loan)}
        ).clone()
    }

    pub fn get_loan_at_location(&self, location: mir::Location) -> facts::Loan {
        // TODO: For aggregates (where two loans are created at the same location) this only finds one loan.
        self.loan_at_position[&location].clone()
    }

    pub fn get_call_loan_at_location(&self, location: mir::Location) -> Option<facts::Loan> {
        self.call_loan_at_position.get(&location).cloned()
    }

    pub fn loan_locations(&self) -> HashMap<facts::Loan, mir::Location> {
        self.loan_position
            .iter()
            .map(|(loan, location)| (*loan, *location))
            .collect()
    }

    /// Convert a facts::Loan to LoanPlaces<'tcx> (if possible)
    pub fn get_loan_places(&self, loan: &facts::Loan)
        -> Result<Option<LoanPlaces<'tcx>>, PlaceRegionsError>
    {
        // Implementing get_loan_places is a bit more complicated when there are tuples. This is
        // because an assignment like
        //   _3 = (move _4, move _5)
        // creates two loans on the same line (if both fields of _3 are references), let's call
        // them L0 and L1. This means we can't just inspect the assign statement to figure out the
        // loan places for L0 -- we also have to find out which field of _3 L0 is associated with.
        // This happens in get_assignment_for_loan, which returns an atomic assignment for a given
        // loan. For the example above, get_assignment_for_loan(L0) would be _3.0 = move _4 and
        // get_assignment_for_loan(L1) would be _3.1 = move _5. Using these atomic assignments, we
        // can simply read off the loan places as before.

        let assignment = if let Some(loan_assignment) = self.get_assignment_for_loan(*loan)? {
            loan_assignment
        } else {
            return Ok(None);
        };
        let (dest, source) = assignment.as_assign().unwrap();
        let dest = dest.clone();
        let source = source.clone();
        let location = self.loan_position[loan];
        Ok(Some(LoanPlaces { dest, source, location }))
    }

    /// Returns the atomic assignment that created a loan. Refer to the documentation of
    /// SplitAggregateAssignment for more information on what an atomic assignment is.
    pub fn get_assignment_for_loan(&self, loan: facts::Loan)
        -> Result<Option<mir::Statement<'tcx>>, PlaceRegionsError>
    {
        let &location = if let Some(loc) = self.loan_position.get(&loan) {
            loc
        } else {
            return Ok(None);
        };
        let stmt = if let Some(s) = self.mir.statement_at(location) {
            s.clone()
        } else {
            return Ok(None);
        };
        let mut assignments: Vec<_> = stmt.split_assignment(self.tcx, &self.mir);

        // TODO: This is a workaround. It seems like some local variables don't have a local
        //  variable declaration in the MIR. One example of this can be observed in the
        //  implementation of the clone method in `prusti/tests/verify/pass/copy/generics.rs`. We
        //  don't have region information for these local variables, which is why the code below
        //  would drop all assignments and return `None`. So if there is only one possible
        //  assignment that may have created a loan, we return it directly without inspecting the
        //  region information. Of course once a local variable that is a tuple is not declared
        //  explicitly, this will fail again.
        if assignments.len() == 1 {
            return Ok(assignments.pop());
        }

        let region = self.get_borrow_region_for_loan(loan);

        // Drops all assignments where the LHS region isn't equal to the region of the loan we're
        // interested in. The reason this works is a bit subtle. First, if execution reaches this
        // point, the original assignment must have been an aggregate assignment, because in the
        // case of other assignments the function returns early in the `assignments.len() == 1`
        // check. For a reference moved into an aggregate via an aggregate assignment, the borrow
        // region immediately associated with the fake loan created for this reference move is the
        // region associated with the left-hand side place this reference is moved into. For
        // example, consider the following assignment:
        //     let _3 = (move _4, move _5);
        // The fake loan created for "move_4" is associated with the region of the place _3.0, and
        // for "move_5" the fake loan is associated with the region of the place _3.1. This
        // correspondence allows us to identify the correct atomic assignment by comparing the
        // region of the left-hand side with the borrow region of the loan. This is hacky, and a
        // solution that does not rely on such subtleties would be much better.
        let mut retained_assignments = vec![];
        for stmt in assignments.into_iter() {
            let (lhs, _) = stmt.as_assign().unwrap_or_else(||
                unreachable!("Borrow starts at statement {:?}", stmt));
            if self.place_regions.for_place(lhs)? == Some(region) {
                retained_assignments.push(stmt);
            }
        };

        Ok(retained_assignments.pop())
    }

    /// Returns the borrow region that requires the terms of the given loan to be enforced. This
    /// does *not* return all regions that require the terms of the loan to be enforced. So for the
    /// following MIR code, it returns the region '2rv but not the region '1rv:
    ///
    /// ```ignore
    /// let _1: &'1rv u32;
    /// _1 = &'2rv 123;
    /// ```
    fn get_borrow_region_for_loan(&self, loan: facts::Loan) -> facts::Region {
        let location = self.get_loan_location(&loan);
        let point = self.get_point(location, PointType::Mid);
        let regions = self.borrowck_in_facts.borrow_region.iter()
            .filter_map(|&(r, l, p)|
                if p == point && l == loan {
                    Some(r)
                } else {
                    None
                })
            .collect::<Vec<_>>();
        if regions.len() == 1 {
            regions[0]
        } else {
            unreachable!(
                "Cannot determine region for loan {:?}, because there is not exactly one possible option: {:?}",
                loan, regions)
        }
    }

    /// Find minimal elements that are the tree roots.
    fn find_loan_roots(&self, loans: &[facts::Loan]) -> Vec<facts::Loan> {
        let mut roots = Vec::new();
        for &loan in loans.iter() {
            let is_smallest = !loans.iter().any(|&other_loan| {
                self.additional_facts
                    .reborrows
                    .contains(&(other_loan, loan))
            });
            debug!("loan={:?} is_smallest={}", loan, is_smallest);
            if is_smallest {
                roots.push(loan);
            }
        }
        roots
    }

    /// Find a variable that has the given region in its type.
    pub fn find_variable(&self, _region: facts::Region) -> Option<mir::Local> {
        // TODO
        None
    }

//     /// Find variable that was moved into the function.
//     pub fn get_moved_variable(&self, kind: &ReborrowingKind) -> mir::Local {
//         match kind {
//             ReborrowingKind::ArgumentMove { ref loan } => {
//                 let index = self
//                     .borrowck_in_facts
//                     .borrow_region
//                     .iter()
//                     .position(|(_, l, _)| l == loan)
//                     .unwrap();
//                 let (region, _, _) = self.borrowck_in_facts.borrow_region[index];
//                 let variable = self.find_variable(region).unwrap();
//                 variable
//             }
//             _ => panic!("This function can be called only with ReborrowingKind::ArgumentMove."),
//         }
//     }

    /// ``loans`` – all loans, including the zombie loans.
    pub fn construct_reborrowing_dag(
        &self,
        loans: &[facts::Loan],
        zombie_loans: &[facts::Loan],
        location: mir::Location,
    ) -> Result<ReborrowingDAG, PoloniusInfoError> {
        self.construct_reborrowing_dag_custom_reborrows(
            loans,
            zombie_loans,
            location,
            &self.additional_facts.reborrows_direct,
        )
    }

    pub fn construct_reborrowing_dag_loop_body(
        &self,
        loans: &[facts::Loan],
        zombie_loans: &[facts::Loan],
        location: mir::Location,
    ) -> Result<ReborrowingDAG, PoloniusInfoError> {
        self.construct_reborrowing_dag_custom_reborrows(
            loans,
            zombie_loans,
            location,
            &self.additional_facts_no_back.reborrows_direct,
        )
    }

    /// Get loops in which loans are defined (if any).
    pub fn get_loan_loops(
        &self,
        loans: &[facts::Loan]
    ) -> Result<Vec<(facts::Loan, mir::BasicBlock)>, PoloniusInfoError> {
        let pairs: Vec<_> = loans
            .iter()
            .flat_map(|loan| {
                let loan_location = if let Some(location) = self.loan_position.get(loan) {
                    location
                } else {
                    // FIXME (Vytautas): This is likely to be wrong.
                    debug!("ERROR: not found for loan: {:?}", loan);
                    return None;
                };
                self.loops
                    .get_loop_head(loan_location.block)
                    .map(|loop_head| (*loan, loop_head))
            })
            .collect();
        for (loan1, loop1) in pairs.iter() {
            let location1 = self.loan_position[loan1];
            for (loan2, loop2) in pairs.iter() {
                let location2 = self.loan_position[loan2];
                if loop1 != loop2 {
                    return Err(PoloniusInfoError::LoansInNestedLoops(
                        location1,
                        *loop1,
                        location2,
                        *loop2
                    ));
                }
            }
        }
        Ok(pairs)
    }

    /// ``loans`` – all loans, including the zombie loans.
    pub fn construct_reborrowing_dag_custom_reborrows(
        &self,
        loans: &[facts::Loan],
        zombie_loans: &[facts::Loan],
        location: mir::Location,
        reborrows_direct: &Vec<(facts::Loan, facts::Loan)>,
    ) -> Result<ReborrowingDAG, PoloniusInfoError> {
        trace!(
            "[enter] construct_reborrowing_dag_custom_reborrows\
             (loans={:?}, zombie_loans={:?}, location={:?})",
            loans,
            zombie_loans,
            location
        );

        let mut loans: Vec<_> = loans.iter().filter(|loan| {
            // FIXME: This is most likely wrong. Filtering out placeholder loans.
            self.borrowck_in_facts.placeholder.iter().all(
                |(_origin, placeholder_loan)| placeholder_loan != *loan
            )
        }).cloned().collect();

        // The representative_loan is a loan that is the root of the
        // reborrowing in some loop. Since it has no proper
        // reborrows_direct relation (because of the cycles), it needs
        // manual treatment in the visit function.
        let mut representative_loan = None;
        if let Some(loop_head) = self.loops.get_loop_head(location.block) {
            let depth = self.loops.get_loop_head_depth(loop_head);
            debug!("loop_head: {:?} depth: {:?}", loop_head, depth);
            // It is fine to have loans defined in an outer loop that is not `loop_head`, because
            // `return` or panic statements might need to jump out of many loops at once.
        } else {
            let loan_loops = self.get_loan_loops(&loans)?;
            if !loan_loops.is_empty() {
                for (loan, loop_head) in loan_loops.iter() {
                    debug!("loan={:?} loop_head={:?}", loan, loop_head);
                }
                let (_, loop_head) = loan_loops[0];
                debug!("loop_head = {:?}", loop_head);
                if self.loop_magic_wands.is_empty() {
                    return Err(PoloniusInfoError::ReborrowingDagHasNoMagicWands(location));
                }
                let loop_magic_wands = &self.loop_magic_wands[&loop_head];
                if loop_magic_wands.len() != 1 {
                    return Err(PoloniusInfoError::MultipleMagicWandsPerLoop(location));
                }
                let magic_wand = &loop_magic_wands[0];
                representative_loan = Some(magic_wand.root_loan);
                if representative_loan.is_none() {
                    return Err(PoloniusInfoError::MagicWandHasNoRepresentativeLoan(location));
                }
                loans = loans
                    .into_iter()
                    .filter(|loan| {
                        !loan_loops.iter().any(|(loop_loan, _)| {
                            loop_loan == loan && Some(*loan) != representative_loan
                        })
                    })
                    .collect();
            }
        }

        // Topologically sort loans.
        let mut sorted_loans = Vec::new();
        let mut permanent_mark = vec![false; loans.len()];
        let mut temporary_mark = vec![false; loans.len()];
        fn visit(
            this: &PoloniusInfo,
            representative_loan: &Option<facts::Loan>,
            reborrows_direct: &Vec<(facts::Loan, facts::Loan)>,
            loans: &[facts::Loan],
            current: usize,
            sorted_loans: &mut Vec<facts::Loan>,
            permanent_mark: &mut Vec<bool>,
            temporary_mark: &mut Vec<bool>,
        ) {
            if permanent_mark[current] {
                return;
            }
            assert!(
                !temporary_mark[current],
                "Not a DAG!\nrepresentative_loan: {:?}\nreborrows_direct: {:?}\nloans: {:?}\ncurrent: {:?}\nsorted_loans: {:?}\npermanent_mark: {:?}\ntemporary_mark: {:?}\nloan_location: {:?}",
                representative_loan,
                reborrows_direct,
                loans,
                current,
                sorted_loans,
                permanent_mark,
                temporary_mark,
                this.loan_position
            );
            temporary_mark[current] = true;
            let current_loan = loans[current];
            if Some(current_loan) == *representative_loan {
                for (new_current, &loan) in loans.iter().enumerate() {
                    if loan == current_loan {
                        // The reborrows relation is reflexive, so we need this check.
                        continue;
                    }
                    if this
                        .additional_facts
                        .reborrows
                        .contains(&(current_loan, loan))
                    {
                        visit(
                            this,
                            representative_loan,
                            reborrows_direct,
                            loans,
                            new_current,
                            sorted_loans,
                            permanent_mark,
                            temporary_mark,
                        );
                    }
                }
            } else {
                for (new_current, &loan) in loans.iter().enumerate() {
                    if Some(loan) == *representative_loan {
                        if this
                            .additional_facts
                            .reborrows
                            .contains(&(current_loan, loan))
                        {
                            visit(
                                this,
                                representative_loan,
                                reborrows_direct,
                                loans,
                                new_current,
                                sorted_loans,
                                permanent_mark,
                                temporary_mark,
                            );
                        }
                    } else {
                        if reborrows_direct.contains(&(current_loan, loan)) {
                            visit(
                                this,
                                representative_loan,
                                reborrows_direct,
                                loans,
                                new_current,
                                sorted_loans,
                                permanent_mark,
                                temporary_mark,
                            );
                        }
                    }
                }
            }
            permanent_mark[current] = true;
            sorted_loans.push(loans[current]);
        }
        loop {
            let index = if let Some(index) = permanent_mark.iter().position(|x| !*x) {
                index
            } else {
                break;
            };
            visit(
                self,
                &representative_loan,
                reborrows_direct,
                &loans,
                index,
                &mut sorted_loans,
                &mut permanent_mark,
                &mut temporary_mark,
            );
        }
        sorted_loans.reverse();
        let nodes: Vec<_> = sorted_loans.iter()
            .map(|&loan| {
                let reborrowing_loans = sorted_loans.iter().cloned()
                    .filter(|&l| self.additional_facts.reborrows_direct.contains(&(l, loan)))
                    .collect::<Vec<_>>();
                let reborrowed_loans = sorted_loans.iter().cloned()
                    .filter(|&l| self.additional_facts.reborrows_direct.contains(&(loan, l)))
                    .collect::<Vec<_>>();
                let kind = self.construct_reborrowing_kind(loan, representative_loan);
                let zombity = self.construct_reborrowing_zombity(
                    loan, &loans, zombie_loans, location);
                let incoming_zombies = self.check_incoming_zombies(
                    loan, &loans, zombie_loans, location);
                ReborrowingDAGNode {
                    loan, kind, zombity, incoming_zombies, reborrowing_loans, reborrowed_loans
                }
            })
            .collect();
        Ok(ReborrowingDAG { nodes: nodes })
    }

    fn construct_reborrowing_kind(
        &self,
        loan: facts::Loan,
        representative_loan: Option<facts::Loan>,
    ) -> ReborrowingKind {
        if let Some(local) = self.call_magic_wands.get(&loan) {
            let region = self.place_regions.for_local(*local).unwrap();
            ReborrowingKind::Call {
                loan: loan,
                variable: *local,
                region: region,
            }
        } else if self.argument_moves.contains(&loan) {
            ReborrowingKind::ArgumentMove { loan: loan }
        } else if Some(loan) == representative_loan {
            for magic_wands in self.loop_magic_wands.values() {
                for magic_wand in magic_wands.iter() {
                    if magic_wand.root_loan == loan {
                        return ReborrowingKind::Loop {
                            magic_wand: (*magic_wand).clone(),
                        };
                    }
                }
            }
            unreachable!("Bug");
        } else {
            ReborrowingKind::Assignment { loan: loan }
        }
    }

    fn construct_reborrowing_zombity(
        &self,
        loan: facts::Loan,
        _loans: &[facts::Loan],
        zombie_loans: &[facts::Loan],
        _location: mir::Location,
    ) -> ReborrowingZombity {
        // Is the loan is a move of a reference, then this source is moved out and,
        // therefore, a zombie.
        let is_reference_move = self.reference_moves.contains(&loan);

        debug!("loan={:?} is_reference_move={:?}", loan, is_reference_move);
        if zombie_loans.contains(&loan) || is_reference_move {
            ReborrowingZombity::Zombie(self.loan_position[&loan])
        } else {
            ReborrowingZombity::Real
        }
    }

    fn check_incoming_zombies(
        &self,
        loan: facts::Loan,
        loans: &[facts::Loan],
        zombie_loans: &[facts::Loan],
        location: mir::Location,
    ) -> bool {
        let incoming_loans: Vec<_> = loans
            .iter()
            .filter(|&&incoming_loan| {
                self.additional_facts
                    .reborrows_direct
                    .contains(&(incoming_loan, loan))
            })
            .map(|incoming_loan| {
                zombie_loans.contains(incoming_loan)
                    || self.reference_moves.contains(incoming_loan)
                    || self.argument_moves.contains(incoming_loan)
            })
            .collect();

        // If a loan is kept alive by a loan that is a call, this means
        // that this loan is an argument to that call and the reference
        // that created it was moved into the call and as a result is a
        // zombie now.
        let has_incoming_call = loans
            .iter()
            .filter(|&&incoming_loan| {
                self.additional_facts
                    .reborrows_direct
                    .contains(&(incoming_loan, loan))
            })
            .any(|incoming_loan| self.call_magic_wands.contains_key(incoming_loan));

        // If a root loan dies at a call, this means that it is kept
        // alive by a reference that was moved into the call and,
        // therefore, its blocking reference is now a zombie.
        let root_die_at_call =
            { is_call(self.mir, location) && self.find_loan_roots(loans).contains(&loan) };

        if root_die_at_call || has_incoming_call {
            true
        } else if !incoming_loans.is_empty() {
            let incoming_zombies = incoming_loans.iter().any(|&b| b);
            debug!(
                "incoming_loans={:?} loan={:?} zombie_loans={:?}",
                incoming_loans, loan, zombie_loans
            );
            assert_eq!(incoming_zombies, incoming_loans.iter().all(|&b| b));
            incoming_zombies
        } else {
            false
        }
    }

//     /// Note: `loans` includes all `zombie_loans`.
//     ///
//     /// This is function is deprecated. Please use
//     /// `construct_reborrowing_dag` instead.
//     pub(super) fn construct_reborrowing_forest(
//         &self,
//         loans: &[facts::Loan],
//         zombie_loans: &[facts::Loan],
//         location: mir::Location,
//     ) -> ReborrowingForest {
//         let roots = self.find_loan_roots(loans);

//         // Reconstruct the tree from each root.
//         let mut trees = Vec::new();
//         for &root in roots.iter() {
//             let tree = ReborrowingTree {
//                 root: self.construct_reborrowing_tree(&loans, zombie_loans, root, location),
//             };
//             trees.push(tree);
//         }

//         let forest = ReborrowingForest { trees: trees };
//         forest
//     }

//     pub(super) fn construct_reborrowing_tree(
//         &self,
//         loans: &[facts::Loan],
//         zombie_loans: &[facts::Loan],
//         node: facts::Loan,
//         location: mir::Location,
//     ) -> ReborrowingNode {
//         let kind = if let Some(local) = self.call_magic_wands.get(&node) {
//             let region = self.variable_regions[&local];
//             ReborrowingKind::Call {
//                 loan: node,
//                 variable: *local,
//                 region: region,
//             }
//         } else {
//             ReborrowingKind::Assignment { loan: node }
//         };
//         let mut children = Vec::new();
//         for &loan in loans.iter() {
//             if self
//                 .additional_facts
//                 .reborrows_direct
//                 .contains(&(node, loan))
//             {
//                 children.push(loan);
//             }
//         }
//         let branching = if children.len() == 1 {
//             let child = children.pop().unwrap();
//             ReborrowingBranching::Single {
//                 child: box self.construct_reborrowing_tree(loans, zombie_loans, child, location),
//             }
//         } else if children.len() > 1 {
//             ReborrowingBranching::Multiple {
//                 children: children
//                     .iter()
//                     .map(|&child| {
//                         self.construct_reborrowing_tree(loans, zombie_loans, child, location)
//                     })
//                     .collect(),
//             }
//         } else {
//             ReborrowingBranching::Leaf
//         };
//         ReborrowingNode {
//             kind: kind,
//             branching: branching,
//             zombity: self.construct_reborrowing_zombity(node, &loans, zombie_loans, location),
//         }
//     }

//     fn add_loop_magic_wand(&mut self, loop_head: mir::BasicBlock, local: mir::Local) -> Result<(), PoloniusInfoError> {
//         let region = self.variable_regions[&local];
//         let magic_wand = LoopMagicWand {
//             loop_id: loop_head,
//             variable: local,
//             region: region,
//             root_loan: self.compute_root_loan(loop_head, local)?,
//         };
//         let entry = self.loop_magic_wands.entry(loop_head).or_insert(Vec::new());
//         entry.push(magic_wand);
//         Ok(())
//     }

//     /// Find the root loan for a specific magic wand.
//     fn compute_root_loan(
//         &mut self, loop_head: mir::BasicBlock, variable: mir::Local
//     ) -> Result<facts::Loan, PoloniusInfoError> {
//         let liveness = self.liveness.get_before_block(loop_head);
//         let mut root_loans = Vec::new();
//         let loop_loans = self.compute_loop_loans(loop_head, variable);
//         for assignment in liveness.iter() {
//             if assignment.target == variable {
//                 for loan in loop_loans.iter() {
//                     debug!("loan: {:?} position: {:?}", loan, self.loan_position[loan]);
//                     if assignment.location == self.loan_position[loan] {
//                         root_loans.push(*loan);
//                     }
//                 }
//             }
//         }
//         if root_loans.len() != 1 {
//             Err(PoloniusInfoError::UnsupportedLoanInLoop {
//                 loop_head,
//                 variable
//             })
//         } else {
//             Ok(root_loans[0])
//         }
//     }

//     /// Find loans created in the loop that are kept alive by the given variable.
//     fn compute_loop_loans(
//         &self,
//         loop_head: mir::BasicBlock,
//         variable: mir::Local,
//     ) -> Vec<facts::Loan> {
//         let location = mir::Location {
//             block: loop_head,
//             statement_index: 0,
//         };
//         let point = facts::Point {
//             location: location,
//             typ: facts::PointType::Start,
//         };
//         let point_index = self.interner.get_point_index(&point);
//         let region = self.variable_regions[&variable];
//         let (all_loans, _) = self.get_all_loans_kept_alive_by(point_index, region);
//         let loop_loans: Vec<_> = all_loans
//             .iter()
//             .filter(|loan| {
//                 let location = self.loan_position[loan];
//                 let loop_body = &self.loops.loop_bodies[&loop_head];
//                 loop_body.contains(&location.block)
//             })
//             .cloned()
//             .collect();
//         loop_loans
//     }

    fn get_successors(&self, location: mir::Location) -> Vec<mir::Location> {
        let statements_len = self.mir[location.block].statements.len();
        if location.statement_index < statements_len {
            vec![mir::Location {
                statement_index: location.statement_index + 1,
                ..location
            }]
        } else {
            let mut successors = Vec::new();
            for successor in self.mir[location.block]
                .terminator
                .as_ref()
                .unwrap()
                .successors()
            {
                successors.push(mir::Location {
                    block: *successor,
                    statement_index: 0,
                });
            }
            successors
        }
    }

//     fn get_predecessors(&self, location: mir::Location) -> Vec<mir::Location> {
//         if location.statement_index > 0 {
//             vec![mir::Location {
//                 statement_index: location.statement_index - 1,
//                 ..location
//             }]
//         } else {
//             debug_assert_eq!(location.statement_index, 0);
//             let mut predecessors = HashSet::new();
//             for (bbi, bb_data) in self.mir.basic_blocks().iter_enumerated() {
//                 for &bb_successor in bb_data.terminator().successors() {
//                     if bb_successor == location.block {
//                         predecessors.insert(mir::Location {
//                             block: bbi,
//                             statement_index: bb_data.statements.len(),
//                         });
//                     }
//                 }
//             }
//             predecessors.into_iter().collect()
//         }
//     }
}

/// Check if the statement is assignment.
fn is_assignment<'tcx>(mir: &mir::Body<'tcx>, location: mir::Location) -> bool {
    let mir::BasicBlockData { ref statements, .. } = mir[location.block];
    if statements.len() == location.statement_index {
        return false;
    }
    match statements[location.statement_index].kind {
        mir::StatementKind::Assign { .. } => true,
        _ => false,
    }
}

/// Check if the terminator is return.
fn is_return<'tcx>(mir: &mir::Body<'tcx>, location: mir::Location) -> bool {
    let mir::BasicBlockData {
        ref statements,
        ref terminator,
        ..
    } = mir[location.block];
    if statements.len() != location.statement_index {
        return false;
    }
    match terminator.as_ref().unwrap().kind {
        mir::TerminatorKind::Return => true,
        _ => false,
    }
}

fn is_call<'tcx>(mir: &mir::Body<'tcx>, location: mir::Location) -> bool {
    let mir::BasicBlockData {
        ref statements,
        ref terminator,
        ..
    } = mir[location.block];
    if statements.len() != location.statement_index {
        return false;
    }
    match terminator.as_ref().unwrap().kind {
        mir::TerminatorKind::Call { .. } => true,
        _ => false,
    }
}

/// Extract the call terminator at the location. Otherwise return None.
fn get_call_destination<'tcx>(
    mir: &mir::Body<'tcx>,
    location: mir::Location,
) -> Option<mir::Place<'tcx>> {
    let mir::BasicBlockData {
        ref statements,
        ref terminator,
        ..
    } = mir[location.block];
    if statements.len() != location.statement_index {
        return None;
    }
    match terminator.as_ref().unwrap().kind {
        mir::TerminatorKind::Call {
            ref destination, ..
        } => {
            if let Some((ref place, _)) = destination {
                Some(place.clone())
            } else {
                None
            }
        }
        ref x => {
            panic!("Expected call, got {:?} at {:?}", x, location);
        }
    }
}

/// Extract reference-typed arguments of the call at the given location.
fn get_call_arguments<'tcx>(mir: &mir::Body<'tcx>, location: mir::Location) -> Vec<mir::Local> {
    let mir::BasicBlockData {
        ref statements,
        ref terminator,
        ..
    } = mir[location.block];
    assert!(statements.len() == location.statement_index);
    match terminator.as_ref().unwrap().kind {
        mir::TerminatorKind::Call { ref args, .. } => {
            let mut reference_args = Vec::new();
            for arg in args {
                match arg {
                    mir::Operand::Copy(place) | mir::Operand::Move(place) => {
                        if place.projection.len() == 0 {
                            reference_args.push(place.local);
                        }
                    }
                    mir::Operand::Constant(_) => {}
                }
            }
            reference_args
        }
        ref x => {
            panic!("Expected call, got {:?} at {:?}", x, location);
        }
    }
}

/// Additional facts derived from the borrow checker facts.
pub struct AdditionalFacts {
    /// A list of loans sorted by id.
    pub loans: Vec<facts::Loan>,
    /// The ``reborrows`` facts are needed for removing “fake” loans: at
    /// a specific program point there are often more than one loan active,
    /// but we are interested in only one of them, which is the original one.
    /// Therefore, we find all loans that are reborrows of the original loan
    /// and remove them. Reborrowing is defined as follows:
    ///
    /// ```datalog
    /// reborrows(Loan, Loan);
    /// reborrows(L1, L2) :-
    ///     borrow_region(R, L1, P),
    ///     restricts(R, P, L2).
    /// reborrows(L1, L3) :-
    ///     reborrows(L1, L2),
    ///     reborrows(L2, L3).
    /// ```
    pub reborrows: Vec<(facts::Loan, facts::Loan)>,
    /// Non-transitive `reborrows`.
    pub reborrows_direct: Vec<(facts::Loan, facts::Loan)>,
    /// The ``zombie_requires`` facts are ``requires`` facts for the loans
    /// that were killed.
    ///
    /// ```datalog
    /// zombie_requires(Region, Loan, Point);
    /// zombie_requires(R, L, Q) :-
    ///     requires(R, L, P),
    ///     killed(L, P),
    ///     cfg_edge(P, Q),
    ///     origin_live_on_entry(R, Q).
    /// zombie_requires(R2, L, P) :-
    ///     zombie_requires(R1, L, P),
    ///     subset(R1, R2, P).
    /// zombie_requires(R, L, Q) :-
    ///     zombie_requires(R, L, P),
    ///     cfg_edge(P, Q),
    ///     origin_live_on_entry(R, Q).
    /// ```
    pub zombie_requires:
        FxHashMap<facts::PointIndex, BTreeMap<facts::Region, BTreeSet<facts::Loan>>>,
    /// The ``zombie_borrow_live_at`` facts are ``borrow_live_at`` facts
    /// for the loans that were killed.
    ///
    /// ```datalog
    /// zombie_borrow_live_at(L, P) :-
    ///     zombie_requires(R, L, P),
    ///     origin_live_on_entry(R, P).
    /// ```
    pub zombie_borrow_live_at: FxHashMap<facts::PointIndex, Vec<facts::Loan>>,
    /// Which loans were killed (become zombies) at a given point.
    pub borrow_become_zombie_at: FxHashMap<facts::PointIndex, Vec<facts::Loan>>,
}

impl AdditionalFacts {
    /// Derive ``zombie_requires``.
    fn derive_zombie_requires(
        all_facts: &facts::AllInputFacts,
        output: &facts::AllOutputFacts,
    ) -> (
        FxHashMap<facts::PointIndex, BTreeMap<facts::Region, BTreeSet<facts::Loan>>>,
        FxHashMap<facts::PointIndex, Vec<facts::Loan>>,
        FxHashMap<facts::PointIndex, Vec<facts::Loan>>,
    ) {
        use self::facts::{Loan, PointIndex as Point, Region};
        use datafrog::{Iteration, Relation};

        let mut iteration = Iteration::new();

        // Variables that are outputs of our computation.
        let zombie_requires = iteration.variable::<(Region, Loan, Point)>("zombie_requires");
        let zombie_borrow_live_at = iteration.variable::<(Loan, Point)>("zombie_borrow_live_at");
        let borrow_become_zombie_at =
            iteration.variable::<(Loan, Point)>("borrow_become_zombie_at");

        // Variables for initial data.
        let requires_lp = iteration.variable::<((Loan, Point), Region)>("requires_lp");
        let killed = iteration.variable::<((Loan, Point), ())>("killed");
        let cfg_edge_p = iteration.variable::<(Point, Point)>("cfg_edge_p");
        let origin_live_on_entry = iteration.variable::<((Region, Point), ())>("origin_live_on_entry");
        let subset_r1p = iteration.variable::<((Region, Point), Region)>("subset_r1p");

        // Temporaries as we perform a multi-way join.
        let zombie_requires_rp = iteration.variable_indistinct("zombie_requires_rp");
        let zombie_requires_p = iteration.variable_indistinct("zombie_requires_p");
        let zombie_requires_1 = iteration.variable_indistinct("zombie_requires_1");
        let zombie_requires_2 = iteration.variable_indistinct("zombie_requires_2");
        let zombie_requires_3 = iteration.variable_indistinct("zombie_requires_3");
        let zombie_requires_4 = iteration.variable_indistinct("zombie_requires_4");

        // Load initial facts.
        requires_lp.insert(Relation::from_iter(output.restricts.iter().flat_map(
            |(&point, region_map)| {
                region_map.iter().flat_map(move |(&region, loans)| {
                    loans.iter().map(move |&loan| ((loan, point), region))
                })
            },
        )));
        killed.insert(Relation::from_iter(
            all_facts
                .killed
                .iter()
                .map(|&(loan, point)| ((loan, point), ())),
        ));
        cfg_edge_p.insert(all_facts.cfg_edge.clone().into());

        let origin_live_on_entry_vec = {
            output.origin_live_on_entry.iter().flat_map(|(point, origins)| {
                let points: Vec<_> = origins.iter().cloned().map(|origin| (origin, *point)).collect();
                points
            })
            // let mut origin_live_on_entry = output.origin_live_on_entry.clone();
            // let all_points: BTreeSet<Point> = all_facts
            //     .cfg_edge
            //     .iter()
            //     .map(|&(p, _)| p)
            //     .chain(all_facts.cfg_edge.iter().map(|&(_, q)| q))
            //     .collect();

            // for &r in &all_facts.universal_region {
            //     for &p in &all_points {
            //          FIXME: Check if already added.
            //         origin_live_on_entry.push((r, p));
            //     }
            // }
            // origin_live_on_entry
        };
        origin_live_on_entry.insert(Relation::from_iter(
            origin_live_on_entry_vec.map(|(r, p)| ((r, p), ())),
        ));
        subset_r1p.insert(Relation::from_iter(output.subset.iter().flat_map(
            |(&point, subset_map)| {
                subset_map.iter().flat_map(move |(&region1, regions)| {
                    regions
                        .iter()
                        .map(move |&region2| ((region1, point), region2))
                })
            },
        )));

        while iteration.changed() {
            zombie_requires_rp.from_map(&zombie_requires, |&(r, l, p)| ((r, p), l));
            zombie_requires_p.from_map(&zombie_requires, |&(r, l, p)| (p, (l, r)));

            // zombie_requires(R, L, Q) :-
            //     requires(R, L, P),
            //     killed(L, P),
            //     cfg_edge(P, Q),
            //     origin_live_on_entry(R, Q).
            zombie_requires_1.from_join(&requires_lp, &killed, |&(l, p), &r, _| (p, (l, r)));
            zombie_requires_2.from_join(&zombie_requires_1, &cfg_edge_p, |&_p, &(l, r), &q| {
                ((r, q), l)
            });
            zombie_requires.from_join(&zombie_requires_2, &origin_live_on_entry, |&(r, q), &l, &()| {
                (r, l, q)
            });
            zombie_requires_4.from_join(&zombie_requires_1, &cfg_edge_p, |&p, &(l, r), &q| {
                ((r, q), (p, l))
            });
            borrow_become_zombie_at.from_join(
                &zombie_requires_4,
                &origin_live_on_entry,
                |_, &(p, l), &()| (l, p),
            );

            // zombie_requires(R2, L, P) :-
            //     zombie_requires(R1, L, P),
            //     subset(R1, R2, P).
            zombie_requires.from_join(&zombie_requires_rp, &subset_r1p, |&(_r1, p), &b, &r2| {
                (r2, b, p)
            });

            // zombie_requires(R, L, Q) :-
            //     zombie_requires(R, L, P),
            //     cfg_edge(P, Q),
            //     origin_live_on_entry(R, Q).
            zombie_requires_3.from_join(&zombie_requires_p, &cfg_edge_p, |&_p, &(l, r), &q| {
                ((r, q), l)
            });
            zombie_requires.from_join(&zombie_requires_3, &origin_live_on_entry, |&(r, q), &l, &()| {
                (r, l, q)
            });

            // zombie_borrow_live_at(L, P) :-
            //     zombie_requires(R, L, P),
            //     origin_live_on_entry(R, P).
            zombie_borrow_live_at.from_join(
                &zombie_requires_rp,
                &origin_live_on_entry,
                |&(_r, p), &l, &()| (l, p),
            );
        }

        let zombie_requires = zombie_requires.complete();
        let mut zombie_requires_map = FxHashMap::default();
        for (region, loan, point) in &zombie_requires.elements {
            zombie_requires_map
                .entry(*point)
                .or_insert(BTreeMap::default())
                .entry(*region)
                .or_insert(BTreeSet::new())
                .insert(*loan);
        }

        let zombie_borrow_live_at = zombie_borrow_live_at.complete();
        let mut zombie_borrow_live_at_map = FxHashMap::default();
        for (loan, point) in &zombie_borrow_live_at.elements {
            zombie_borrow_live_at_map
                .entry(*point)
                .or_insert(Vec::new())
                .push(*loan);
        }

        let borrow_become_zombie_at = borrow_become_zombie_at.complete();
        let mut borrow_become_zombie_at_map = FxHashMap::default();
        for (loan, point) in &borrow_become_zombie_at.elements {
            borrow_become_zombie_at_map
                .entry(*point)
                .or_insert(Vec::new())
                .push(*loan);
        }

        (
            zombie_requires_map,
            zombie_borrow_live_at_map,
            borrow_become_zombie_at_map,
        )
    }

    /// Derive additional facts from the borrow checker facts.
    pub fn new(
        all_facts: &facts::AllInputFacts,
        output: &facts::AllOutputFacts,
        incompatible_loans: &[Vec<facts::Loan>]
    ) -> AdditionalFacts {
        let (zombie_requires, zombie_borrow_live_at, borrow_become_zombie_at) =
            Self::derive_zombie_requires(all_facts, output);

        let restricts = output.restricts.iter().chain(zombie_requires.iter());
        let borrow_regions = all_facts.borrow_region.iter();
        let reborrows = Self::load_reborrows(restricts, borrow_regions, incompatible_loans);

        let mut reborrows = Self::transitive_closure(reborrows);

        // Remove reflexive edges.
        reborrows.retain(|(l1, l2)| l1 != l2);

        // A non-transitive version of reborrows.
        let reborrows_direct = Self::derive_nontransitive(&reborrows);

        // Compute the sorted list of all loans.
        let mut loans: Vec<_> = all_facts.borrow_region.iter().map(|&(_, l, _)| l).collect();
        loans.sort();

        AdditionalFacts {
            loans, reborrows, reborrows_direct,
            zombie_requires, zombie_borrow_live_at, borrow_become_zombie_at
        }
    }

    fn load_reborrows<'a>(
        restricts: impl Iterator<Item=(
            &'a facts::PointIndex,
            &'a BTreeMap<facts::Region, BTreeSet<facts::Loan>>
        )>,
        borrow_regions: impl Iterator<Item=&'a (
            facts::Region,
            facts::Loan,
            facts::PointIndex
        )>,
        incompatible_loans: &[Vec<facts::Loan>]
    ) -> Vec<(facts::Loan, facts::Loan)> {
        use self::facts::{Loan, PointIndex as Point, Region};

        let mut iteration = datafrog::Iteration::new();

        // Variables that are outputs of our computation.
        let v_reborrows = iteration.variable::<(Loan, Loan)>("reborrows");

        // Variables for initial data.
        let v_restricts = iteration.variable::<((Point, Region), Loan)>("restricts");
        let v_borrow_region = iteration.variable::<((Point, Region), Loan)>("borrow_region");

        // Load initial data.
        let restricts_items = restricts.flat_map(|(&point, region_map)|
            region_map.iter().flat_map(move |(&region, loans)|
                loans.iter().map(move |&loan| ((point, region), loan))));
        v_restricts.insert(datafrog::Relation::from_iter(restricts_items));

        let borrow_region_items = borrow_regions.map(|&(r, l, p)| ((p, r), l));
        v_borrow_region.insert(datafrog::Relation::from_iter(borrow_region_items));

        while iteration.changed() {
            // reborrows(L1, L2) :-
            //   borrow_region(R, L1, P),
            //   restricts(R, P, L2).
            v_reborrows.from_join(&v_borrow_region, &v_restricts, |_, &l1, &l2| (l1, l2));
        }

        let mut reborrows = v_reborrows.complete().elements;

        // Remove incompatible reborrows. This is necessary because a tuple assignment like
        //   let _3 = (move _4, move _5);
        // creates two (fake) loans (if the fields of _3 are references) that are associated with
        // the same program location. Assume these two loans are L0 and L1 and include regions R1
        // and R2, respectively. When R1 and R2 must be equal due to later constraints in the
        // program, both R1 and R2 restrict both loans (as defined by the restricts relation). This
        // means we will infer reborrows(L1, L2) and reborrows(L2, L1), even though neither loan
        // reborrows the other. This would not happen if L1 and L2 were created at different
        // program locations, which is why this problem is unique to references in tuples.
        reborrows.retain(|(l1, l2)|
            !incompatible_loans.iter().any(|s| s.contains(l1) && s.contains(l2)));

        reborrows
    }

    fn transitive_closure(
        reborrows: Vec<(facts::Loan, facts::Loan)>
    ) -> Vec<(facts::Loan, facts::Loan)> {
        let mut iteration = datafrog::Iteration::new();

        let v_reborrows = iteration.variable("reborrows");
        v_reborrows.insert(reborrows.into());

        let v_reborrows_1 = iteration.variable_indistinct("reborrows_1");
        let v_reborrows_2 = iteration.variable_indistinct("reborrows_2");

        // Compute transitive closure of reborrows:
        // reborrows(L1, L3) :-
        //   reborrows(L1, L2),
        //   reborrows(L2, L3).
        while iteration.changed() {
            v_reborrows_1.from_map(&v_reborrows, |&(l1, l2)| (l2, l1));
            v_reborrows_2.from_map(&v_reborrows, |&(l2, l3)| (l2, l3));
            v_reborrows.from_join(&v_reborrows_1, &v_reborrows_2, |_, &l1, &l3| (l1, l3));
        }

        v_reborrows.complete().elements
    }

    fn derive_nontransitive(
        reborrows: &[(facts::Loan, facts::Loan)]
    ) -> Vec<(facts::Loan, facts::Loan)> {
        reborrows.iter().cloned()
            .filter(|&(l1, l2)|
                !reborrows.iter().any(|&(l3, l4)|
                    l4 == l2 && reborrows.contains(&(l1, l3))))
            .collect()
    }
}
