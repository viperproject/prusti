use std::cmp::Ordering;
use std::hash::Hash;
use std::hash::Hasher;

pub(super) use lift::LiftBindings;
use prusti_common::vir;

use super::super::Context;

mod partition;
mod lift;

/// This represents a single binding of a pledge, magic wand, or expiration tool. For example,
/// consider the following (encoded) pledge:
/// ```ignore
/// old[before_expiry](_0.tuple_0.val_ref.val_int) == old[after_unblocked](_1.val_ref.f$x.val_int)
/// ```
/// Because we must manually evaluate the two `old` expressions at the right times, they are pulled
/// out of the encoded pledge. This gives the following expression:
/// ```ignore
/// pledge$0 == pledge$1
/// ```
/// With the following bindings:
///  - `pledge$0 // BeforeExpiry // _0.tuple_0.val_ref.val_int`
///  - `pledge$1 // AfterUnblocked // _1.val_ref.f$x.val_int`
#[derive(Clone)]
pub struct Binding(pub vir::LocalVar, pub Context, pub vir::Expr);

impl PartialEq for Binding {
    fn eq(&self, other: &Self) -> bool {
        let Binding(v1, _, _) = self;
        let Binding(v2, _, _) = other;
        v1 == v2
    }
}

impl Eq for Binding {}

impl Hash for Binding {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Binding(v, _, _) = self;
        v.hash(state);
    }
}

impl Ord for Binding {
    fn cmp(&self, other: &Self) -> Ordering {
        let Binding(v1, _, _) = self;
        let Binding(v2, _, _) = other;
        v1.name.cmp(&v2.name)
    }
}

impl PartialOrd for Binding {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub(super) fn encode_binding(
    Binding(var, context, expr): Binding,
    lhs_label: &str
) -> (vir::LocalVar, vir::Expr) {
    use super::super::Context::*;
    let expr = match context {
        BeforeExpiry => expr.old(lhs_label),
        AfterUnblocked => expr
    };
    (var, expr)
}
