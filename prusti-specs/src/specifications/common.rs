//! Data structures for storing information about specifications.
//!
//! Please see the `parser.rs` file for more information about
//! specifications.

use serde::{Deserialize, Serialize};
use std::convert::TryFrom;
use std::fmt::{Display, Debug};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// A specification type.
pub enum SpecType {
    /// Precondition of a procedure.
    Precondition,
    /// Postcondition of a procedure.
    Postcondition,
    /// Loop invariant or struct invariant
    Invariant,
}

#[derive(Debug)]
/// A conversion from string into specification type error.
pub enum TryFromStringError {
    /// Reported when the string being converted is not one of the
    /// following: `requires`, `ensures`, `invariant`.
    UnknownSpecificationType,
}

impl<'a> TryFrom<&'a str> for SpecType {
    type Error = TryFromStringError;

    fn try_from(typ: &str) -> Result<SpecType, TryFromStringError> {
        match typ {
            "requires" => Ok(SpecType::Precondition),
            "ensures" => Ok(SpecType::Postcondition),
            "invariant" => Ok(SpecType::Invariant),
            _ => Err(TryFromStringError::UnknownSpecificationType),
        }
    }
}

#[derive(
    Debug, Default, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize, PartialOrd, Ord,
)]
/// A unique ID of the specification element such as entire precondition
/// or postcondition.
pub struct SpecificationId(Uuid);

impl Display for SpecificationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            self.0.to_simple().encode_lower(&mut Uuid::encode_buffer()),
        )
    }
}

impl std::convert::TryFrom<String> for SpecificationId {
    type Error = uuid::Error;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Uuid::parse_str(&value).map(|id| Self(id))
    }
}

impl SpecificationId {
    pub fn dummy() -> Self {
        Self(Uuid::nil())
    }
}

pub(crate) struct SpecificationIdGenerator {}

impl SpecificationIdGenerator {
    pub(crate) fn new() -> Self {
        Self {}
    }
    pub(crate) fn generate(&mut self) -> SpecificationId {
        SpecificationId(Uuid::new_v4())
    }
}

#[derive(Debug, Default, PartialEq, Eq, Hash, Clone, Copy, Serialize, Deserialize)]
/// A unique ID of the Rust expression used in the specification.
pub struct ExpressionId(u64);

impl Display for ExpressionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0,)
    }
}

pub(crate) struct ExpressionIdGenerator {
    last_id: u64,
}

impl ExpressionIdGenerator {
    pub(crate) fn new() -> Self {
        Self { last_id: 100 }
    }
    pub(crate) fn generate(&mut self) -> ExpressionId {
        self.last_id += 1;
        ExpressionId(self.last_id)
    }
}

impl Into<u64> for ExpressionId {
    fn into(self) -> u64 {
        self.0
    }
}

pub(crate) struct NameGenerator {}

impl NameGenerator {
    pub(crate) fn new() -> Self { Self { } }
    pub(crate) fn generate_struct_name(&self, item: &syn::ItemImpl) -> Result<String, String> {
        let mut path_str: String = String::new();

        match &*item.self_ty {
            syn::Type::Path (ty_path) => {
                for seg in ty_path.path.segments.iter() {
                    path_str.push_str(&seg.ident.to_string());
                }
            }
            _ => {
                return Err("expected a path".to_string());
            }
        };
        let uuid = Uuid::new_v4().to_simple();

        Ok(format!("PrustiStruct{}{}", path_str, uuid))
    }

    pub(crate) fn generate_mod_name(&self, ident: &syn::Ident) -> String {
        let uuid = Uuid::new_v4().to_simple();
        format!("{}{}", ident.to_string(), uuid)
    }
}

#[derive(Debug, Clone)]
/// A Rust expression used in the specification.
pub struct Expression<EID, ET> {
    /// Identifier of the specification to which this expression belongs.
    pub spec_id: SpecificationId,
    /// Unique identifier.
    pub id: EID,
    /// Actual expression.
    pub expr: ET,
}

#[derive(Debug, Clone)]
/// An assertion used in the specification.
pub struct Assertion<EID, ET, AT> {
    /// Subassertions.
    pub kind: Box<AssertionKind<EID, ET, AT>>,
}

#[derive(Debug, Clone)]
/// A single trigger for a quantifier.
pub struct Trigger<EID, ET>(pub Vec<Expression<EID, ET>>);

impl<EID, ET> Trigger<EID, ET> {
    /// Construct a new trigger, which is a “conjunction” of terms.
    pub fn new(terms: Vec<Expression<EID, ET>>) -> Trigger<EID, ET> {
        Trigger(terms)
    }
    /// Getter for terms.
    pub fn terms(&self) -> &Vec<Expression<EID, ET>> {
        &self.0
    }
}

impl<EID, ET> IntoIterator for Trigger<EID, ET> {
    type Item = Expression<EID, ET>;
    type IntoIter = ::std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Debug, Clone)]
/// A set of triggers used in the quantifier.
pub struct TriggerSet<EID, ET>(pub Vec<Trigger<EID, ET>>);

impl<EID, ET> TriggerSet<EID, ET> {
    /// Construct a new trigger set.
    pub fn new(triggers: Vec<Trigger<EID, ET>>) -> TriggerSet<EID, ET> {
        TriggerSet(triggers)
    }
    /// Getter for triggers.
    pub fn triggers(&self) -> &Vec<Trigger<EID, ET>> {
        &self.0
    }
}

impl<EID, ET> IntoIterator for TriggerSet<EID, ET> {
    type Item = Trigger<EID, ET>;
    type IntoIter = ::std::vec::IntoIter<Self::Item>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[derive(Debug, Clone)]
/// A sequence of variables used in the forall.
pub struct ForAllVars<EID, AT> {
    /// Identifier of the specification to which this sequence of variables
    /// belongs.
    pub spec_id: SpecificationId,
    /// Unique id for this sequence of variables.
    pub id: EID,
    /// Variables.
    pub vars: Vec<AT>,
}

#[derive(Debug, Clone)]
/// An assertion kind used in the specification.
pub enum AssertionKind<EID, ET, AT> {
    /// A single Rust expression.
    Expr(Expression<EID, ET>),
    /// Conjunction &&.
    And(Vec<Assertion<EID, ET, AT>>),
    /// Implication ==>
    Implies(Assertion<EID, ET, AT>, Assertion<EID, ET, AT>),
    /// TODO < Even > ==> x % 2 == 0
    TypeCond(ForAllVars<EID, AT>, Assertion<EID, ET, AT>),
    /// Quantifier
    ForAll(
        ForAllVars<EID, AT>,
        TriggerSet<EID, ET>,
        Assertion<EID, ET, AT>,
    ),
}

#[derive(Debug, Clone)]
/// Pledge `pledge(rhs)`
pub struct Pledge<EID, ET, AT> {
    /// The body rhs.
    pub rhs: Assertion<EID, ET, AT>,
}

#[derive(Debug, Clone)]
/// Specification such as precondition, postcondition, or invariant.
pub struct Specification<EID, ET, AT> {
    /// Specification type.
    pub typ: SpecType,
    /// Actual specification.
    pub assertion: Assertion<EID, ET, AT>,
}

/// Specification of a loop.
#[derive(Debug, Clone)]
pub struct LoopSpecification<EID, ET, AT> {
    /// Loop invariant.
    pub invariant: Vec<Assertion<EID, ET, AT>>,
}

impl<EID, ET, AT> LoopSpecification<EID, ET, AT> {
    pub fn new(invariant: Vec<Assertion<EID, ET, AT>>) -> Self {
        Self { invariant }
    }
    pub fn empty() -> Self {
        Self::new(Vec::new())
    }
    pub fn is_empty(&self) -> bool {
        self.invariant.is_empty()
    }
}

/// Specification of a procedure.
#[derive(Debug, Clone)]
pub struct ProcedureSpecification<EID, ET, AT> {
    /// Precondition.
    pub pres: Vec<Assertion<EID, ET, AT>>,
    /// Postcondition.
    pub posts: Vec<Assertion<EID, ET, AT>>,
    /// Pledges in the postcondition.
    pub pledges: Vec<Pledge<EID, ET, AT>>,
}

impl<EID, ET, AT> ProcedureSpecification<EID, ET, AT> {
    pub fn new(
        pres: Vec<Assertion<EID, ET, AT>>,
        posts: Vec<Assertion<EID, ET, AT>>,
        pledges: Vec<Pledge<EID, ET, AT>>
    ) -> Self {
        Self { pres, posts, pledges }
    }
    pub fn empty() -> Self {
        Self::new(Vec::new(), Vec::new(), Vec::new())
    }
    pub fn is_empty(&self) -> bool {
        self.pres.is_empty() && self.posts.is_empty()
    }
}

#[derive(Debug, Clone)]
/// Specification of a single element such as procedure or loop.
pub enum SpecificationSet<EID, ET, AT> {
    /// (Precondition, Postcondition)
    Procedure(ProcedureSpecification<EID, ET, AT>),
    /// Loop invariant.
    Loop(LoopSpecification<EID, ET, AT>),
    /// Struct invariant.
    Struct(Vec<Specification<EID, ET, AT>>),
}

impl<EID, ET, AT> SpecificationSet<EID, ET, AT> {
    pub fn is_empty(&self) -> bool {
        match self {
            SpecificationSet::Procedure(spec) => spec.is_empty(),
            SpecificationSet::Loop(ref invs) => invs.is_empty(),
            SpecificationSet::Struct(ref invs) => invs.is_empty(),
        }
    }
}

impl<EID: Clone + Debug, ET: Clone + Debug, AT: Clone + Debug> SpecificationSet<EID, ET, AT> {
    /// Trait implementation method refinement
    /// Choosing alternative C as discussed in
    /// https://ethz.ch/content/dam/ethz/special-interest/infk/chair-program-method/pm/documents/Education/Theses/Matthias_Erdin_MA_report.pdf
    /// pp 19-23
    ///
    /// In other words, any pre-/post-condition provided by `other` will overwrite any provided by
    /// `self`.
    pub fn refine(&self, other: &Self) -> Self {
        let mut pres = vec![];
        let mut posts = vec![];
        let mut pledges = vec![];
        let (ref_pre, ref_post, ref_pledges) = {
            if let SpecificationSet::Procedure(ProcedureSpecification { ref pres, ref posts, ref pledges}) = other {
                (pres, posts, pledges)
            } else {
                unreachable!("Unexpected: {:?}", other)
            }
        };
        let (base_pre, base_post, base_pledges) = {
            if let SpecificationSet::Procedure(ProcedureSpecification { ref pres, ref posts, ref pledges}) = self {
                (pres, posts, pledges)
            } else {
                unreachable!("Unexpected: {:?}", self)
            }
        };
        if ref_pre.is_empty() {
            pres.append(&mut base_pre.clone());
        } else {
            pres.append(&mut ref_pre.clone());
        }
        if ref_post.is_empty() {
            posts.append(&mut base_post.clone());
        } else {
            posts.append(&mut ref_post.clone());
        }
        if ref_pledges.is_empty() {
            pledges.append(&mut base_pledges.clone());
        } else {
            pledges.append(&mut ref_pledges.clone());
        }
        SpecificationSet::Procedure(ProcedureSpecification { pres, posts, pledges })
    }

}