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
    /// Predicate
    Predicate,
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
            "predicate" => Ok(SpecType::Predicate),
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

/// A reference to a procedure specification.
#[derive(Debug, Clone, Copy)]
pub enum SpecIdRef {
    Precondition(SpecificationId),
    Postcondition(SpecificationId),
    Pledge {
        lhs: Option<SpecificationId>,
        rhs: SpecificationId,
    },
    Predicate(SpecificationId),
}

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
        Uuid::parse_str(&value).map(Self)
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

impl From<ExpressionId> for u64 {
    fn from(id: ExpressionId) -> Self {
        id.0
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
/// Variables used in a forall.
pub struct ForAllVars<EID, AT> {
    /// Identifier of the specification to which these variables belongs.
    pub spec_id: SpecificationId,
    /// Unique id for this sequence of variables.
    pub id: EID,
    /// Variables.
    pub vars: Vec<AT>,
}

#[derive(Debug, Clone)]
/// Variables used in a specification entailment.
pub struct SpecEntailmentVars<EID, AT> {
    /// Identifier of the specification to which these variables belongs.
    pub spec_id: SpecificationId,
    /// Unique id of the pre state.
    pub pre_id: EID,
    /// Unique id of the post state.
    pub post_id: EID,
    /// Variables.
    pub args: Vec<AT>,
    pub result: AT,
}

#[derive(Debug, Clone)]
/// A variable raised to a nonnegative power used in credit polynomials
pub struct CreditVarPower<EID, VT> {
    /// Identifier of the specification to which this power belongs.
    pub spec_id: SpecificationId,
    /// Unique id for this power.
    pub id: EID,
    /// variable at the base of the exponentiation
    pub var: VT,
    pub exponent: u32,
}

#[derive(Debug, Clone)]
/// One term of a credit polynomial, e.g. 3*n^2m^1
pub struct CreditPolynomialTerm<EID, ET, VT> {
    /// constant coefficient expression, possibly containing identifiers
    /// for cost function coefficients of called functions
    pub coeff_expr: Expression<EID, ET>,
    /// Product/vector of variables raised to nonnegative powers
    pub powers: Vec<CreditVarPower<EID, VT>>,
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
    /// Specification entailment
    SpecEntailment {
        closure: Expression<EID, ET>,
        arg_binders: SpecEntailmentVars<EID, AT>,
        pres: Vec<Assertion<EID, ET, AT>>,
        posts: Vec<Assertion<EID, ET, AT>>,
    },
    CreditPolynomial {
        /// Identifier of the specification to which this credit polynomial belongs
        spec_id: SpecificationId,
        /// Id for this polynomial.
        id: EID,
        credit_type: String,    //TODO: enum later
        concrete_terms: Vec<CreditPolynomialTerm<EID, ET, AT>>,          // TODO: replace AT? is not really an argument declaration
        abstract_terms: Vec<CreditPolynomialTerm<EID, ET, AT>>,
    },
}

/// Folder to transform an assertion
pub trait AssertionFolder<EID, ET, AT> {
    fn fold_assertion(&mut self, a: Assertion<EID, ET, AT>) -> Assertion<EID, ET, AT> {
        Assertion { kind: box self.fold_kind(*a.kind) }
    }
    fn fold_kind(&mut self, kind: AssertionKind<EID, ET, AT>) -> AssertionKind<EID, ET, AT> {
        match kind {
            AssertionKind::Expr(e) => self.fold_expr(e),
            AssertionKind::And(vec) => self.fold_and(vec),
            AssertionKind::Implies(left, right) => self.fold_implies(left, right),
            AssertionKind::TypeCond(vars, a) => self.fold_type_cond(vars, a),
            AssertionKind::ForAll(vars, t, a) => self.fold_for_all(vars, t, a),
            AssertionKind::SpecEntailment {closure, arg_binders, pres, posts} => {
                self.fold_spec_entailment(closure, arg_binders, pres, posts)
            },
            AssertionKind::CreditPolynomial {spec_id, id, credit_type, concrete_terms, abstract_terms} => {
                self.fold_credit_polynomial(spec_id, id, credit_type, concrete_terms, abstract_terms)
            },
        }
    }
    fn fold_expr(&mut self, expression: Expression<EID, ET>) -> AssertionKind<EID, ET, AT> {
        AssertionKind::Expr(self.fold_expression(expression))
    }
    fn fold_and(&mut self, assertions: Vec<Assertion<EID, ET, AT>>) -> AssertionKind<EID, ET, AT> {
        AssertionKind::And(assertions.into_iter().map(|a| self.fold_assertion(a)).collect())
    }
    fn fold_implies(&mut self, left: Assertion<EID, ET, AT>, right: Assertion<EID, ET, AT>) -> AssertionKind<EID, ET, AT> {
        AssertionKind::Implies(self.fold_assertion(left), self.fold_assertion(right))
    }
    fn fold_type_cond(&mut self, vars: ForAllVars<EID, AT>, assertion: Assertion<EID, ET, AT>) -> AssertionKind<EID, ET, AT> {
        AssertionKind::TypeCond(vars, self.fold_assertion(assertion))
    }
    fn fold_for_all(
        &mut self,
        vars: ForAllVars<EID, AT>,
        triggers: TriggerSet<EID, ET>,
        assertion: Assertion<EID, ET, AT>
    ) -> AssertionKind<EID, ET, AT> {
        AssertionKind::ForAll(vars, triggers, self.fold_assertion(assertion))
    }
    fn fold_spec_entailment(
        &mut self,
        closure: Expression<EID, ET>,
        arg_binders: SpecEntailmentVars<EID, AT>,
        pres: Vec<Assertion<EID, ET, AT>>,
        posts: Vec<Assertion<EID, ET, AT>>
    ) -> AssertionKind<EID, ET, AT> {
        AssertionKind::SpecEntailment {
            closure: self.fold_expression(closure),
            arg_binders,
            pres: pres.into_iter().map(|a| self.fold_assertion(a)).collect(),
            posts: posts.into_iter().map(|a| self.fold_assertion(a)).collect()
        }
    }
    fn fold_credit_polynomial(
        &mut self,
        spec_id: SpecificationId,
        id: EID,
        credit_type: String,
        concrete_terms: Vec<CreditPolynomialTerm<EID, ET, AT>>,
        abstract_terms: Vec<CreditPolynomialTerm<EID, ET, AT>>,
    ) -> AssertionKind<EID, ET, AT> {
        AssertionKind::CreditPolynomial {
            spec_id,
            id,
            credit_type,
            concrete_terms,
            abstract_terms,
        }
    }
    fn fold_expression(&mut self, e: Expression<EID, ET>) -> Expression<EID, ET> {
        e
    }
}

#[derive(Debug, Clone)]
/// Pledge `after_expiry(ref => rhs)`
///     or `after_expiry_if(ref => lhs, rhs)`
pub struct Pledge<EID, ET, AT> {
    /// The ref.
    pub reference: Option<Expression<EID, ET>>,
    /// The body lhs.
    pub lhs: Option<Assertion<EID, ET, AT>>,
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
    /// Preconditions.
    pub pres: Vec<Assertion<EID, ET, AT>>,
    /// Postconditions.
    pub posts: Vec<Assertion<EID, ET, AT>>,
    /// Pledges in the postcondition.
    pub pledges: Vec<Pledge<EID, ET, AT>>,

    /// Body of a `predicate!` function if this assertion originates from one.
    /// This will be encoded to viper instead of the translation from Rust
    /// otherwise.
    pub predicate_body: Option<Assertion<EID, ET, AT>>,

    pub pure: bool,
    pub trusted: bool,
}

impl<EID, ET, AT> ProcedureSpecification<EID, ET, AT> {
    pub fn new(
        pres: Vec<Assertion<EID, ET, AT>>,
        posts: Vec<Assertion<EID, ET, AT>>,
        pledges: Vec<Pledge<EID, ET, AT>>,
        predicate_body: Option<Assertion<EID, ET, AT>>,
    ) -> Self {
        Self {
            pres,
            posts,
            pledges,
            predicate_body,
            pure: false,
            trusted: false,
        }
    }
    pub fn empty() -> Self {
        Self::new(Vec::new(), Vec::new(), Vec::new(), None)
    }
    pub fn is_empty(&self) -> bool {
        // TODO: should pledges be here as well?
        self.pres.is_empty() && self.posts.is_empty() && self.predicate_body.is_none()
    }
}

impl<EID: Clone + Debug, ET: Clone + Debug, AT: Clone + Debug> ProcedureSpecification<EID, ET, AT> {
    /// Trait implementation method refinement
    /// Choosing alternative C as discussed in
    /// https://ethz.ch/content/dam/ethz/special-interest/infk/chair-program-method/pm/documents/Education/Theses/Matthias_Erdin_MA_report.pdf
    /// pp 19-23
    ///
    /// In other words, any pre-/post-condition provided by `other` will overwrite any provided by
    /// `self`.
    pub fn refine(&self, other: &Self) -> Self {
        let pres = if other.pres.is_empty() {
            self.pres.clone()
        } else {
            other.pres.clone()
        };
        let posts = if other.posts.is_empty() {
            self.posts.clone()
        } else {
            other.posts.clone()
        };
        let pledges = if other.pledges.is_empty() {
            self.pledges.clone()
        } else {
            other.pledges.clone()
        };
        let predicate_body = if other.predicate_body.is_none() {
            self.predicate_body.clone()
        } else {
            other.predicate_body.clone()
        };
        Self {
            pres,
            posts,
            pledges,
            predicate_body,
            pure: other.pure,
            trusted: other.trusted,
        }
    }
}

#[derive(Debug, Clone)]
/// Specification of a single element such as procedure or loop.
pub enum SpecificationSet<EID, ET, AT> {
    /// Procedure specifications (preconditions, postconditions, pledges, attributes).
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
    #[track_caller]
    pub fn expect_procedure(&self) -> &ProcedureSpecification<EID, ET, AT> {
        if let SpecificationSet::Procedure(spec) = self {
            return spec;
        }
        unreachable!("expected Procedure: {:?}", self);
    }

    #[track_caller]
    pub fn expect_mut_procedure(&mut self) -> &mut ProcedureSpecification<EID, ET, AT> {
        if let SpecificationSet::Procedure(spec) = self {
            return spec;
        }
        unreachable!("expected Procedure: {:?}", self);
    }

    #[track_caller]
    pub fn expect_loop(&self) -> &LoopSpecification<EID, ET, AT> {
        if let SpecificationSet::Loop(spec) = self {
            return spec;
        }
        unreachable!("expected Loop: {:?}", self);
    }

    #[track_caller]
    pub fn expect_struct(&self) -> &Vec<Specification<EID, ET, AT>> {
        if let SpecificationSet::Struct(spec) = self {
            return spec;
        }
        unreachable!("expected Struct: {:?}", self);
    }
}
