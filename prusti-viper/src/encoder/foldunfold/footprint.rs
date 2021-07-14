// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::places_utils::{union, union3};
use crate::encoder::foldunfold::perm::Perm::*;
use crate::encoder::foldunfold::perm::*;
use crate::encoder::foldunfold::requirements::*;
use prusti_common::vir;
use prusti_common::vir::PermAmount;
use std::collections::HashMap;
use std::collections::HashSet;
use std::iter::FromIterator;
use log::{trace, debug};

pub trait ExprFootprintGetter {
    /// Returns the precise footprint of an expression, that is the permissions that must be
    /// added/removed when executing an `inhale/exhale expr` statement.
    fn get_footprint(&self, predicates: &HashMap<String, vir::Predicate>) -> HashSet<Perm>;
}

impl ExprFootprintGetter for vir::Expr {
    fn get_footprint(&self, predicates: &HashMap<String, vir::Predicate>) -> HashSet<Perm> {
        trace!("get_footprint {}", self);
        let res = match self {
            vir::Expr::Local(_, _)
            | vir::Expr::Field(_, _, _)
            | vir::Expr::Variant(_, _, _)
            | vir::Expr::AddrOf(_, _, _)
            | vir::Expr::LabelledOld(_, _, _)
            | vir::Expr::Const(_, _)
            | vir::Expr::FuncApp(..)
            | vir::Expr::DomainFuncApp(..)
            | vir::Expr::InhaleExhale(..) => HashSet::new(),

            vir::Expr::Unfolding(_, args, expr, perm_amount, variant, _) => {
                assert_eq!(args.len(), 1);
                let place = &args[0];
                debug_assert!(place.is_place());

                // We want to temporarly unfold place
                let predicate_name = place.typed_ref_name().unwrap();
                let predicate = predicates.get(&predicate_name).unwrap();

                let pred_self_place: vir::Expr = predicate.self_place();
                let places_in_pred: HashSet<Perm> = predicate
                    .get_body_footprint(variant)
                    .into_iter()
                    .map(|aop| {
                        aop.map_place(|p| p.replace_place(&pred_self_place, place))
                            .init_perm_amount(*perm_amount)
                    })
                    .collect();

                // Simulate temporary unfolding of `place`
                let expr_access_places = expr.get_footprint(predicates);

                // inhaled = inhaled in body - unfolding
                perm_difference(expr_access_places, places_in_pred)
            }

            vir::Expr::UnaryOp(_, ref expr, _) => expr.get_footprint(predicates),

            vir::Expr::BinOp(_, box left, box right, _) => union(
                &left.get_footprint(predicates),
                &right.get_footprint(predicates),
            ),

            vir::Expr::ContainerOp(_, box left, box right, _) => union(
                &left.get_footprint(predicates),
                &right.get_footprint(predicates),
            ),

            vir::Expr::Seq(_, elems, _) => {
                elems
                    .iter()
                    .map(|e| {
                        e.get_footprint(predicates)
                    })
                    .fold(HashSet::new(), |mut hs, fp| {
                        hs.extend(fp); hs
                    })
            }

            vir::Expr::Cond(box guard, box left, box right, _) => union3(
                &guard.get_footprint(predicates),
                &left.get_footprint(predicates),
                &right.get_footprint(predicates),
            ),

            vir::Expr::ForAll(vars, _triggers, box body, _) => {
                assert!(vars.iter().all(|var| !var.typ.is_ref()));
                let vars_places: HashSet<Perm> = vars
                    .iter()
                    .map(|var| Acc(vir::Expr::local(var.clone()), PermAmount::Write))
                    .collect();
                perm_difference(body.get_footprint(predicates), vars_places)
            }

            vir::Expr::PredicateAccessPredicate(_, box ref arg, perm_amount, _) => {
                let opt_perm = if arg.is_place() {
                    Some(match arg.get_label() {
                        None => Perm::Pred(arg.clone(), *perm_amount),
                        Some(label) => Perm::Pred(arg.clone().old(label), *perm_amount),
                    })
                } else {
                    None
                };

                opt_perm.into_iter().collect()
            }

            vir::Expr::CreditAccessPredicate(_, _args, _frac_perm_amount, _) => HashSet::new(),       //TODO: ?

            vir::Expr::FieldAccessPredicate(box ref place, perm_amount, _) => {
                // In Prusti we assume to have only places here
                debug_assert!(place.is_place());
                debug_assert!(place.is_curr());

                let perm = Acc(place.clone(), *perm_amount);

                Some(perm).into_iter().collect()
            }

            vir::Expr::MagicWand(ref _lhs, ref _rhs, _borrow, _) => {
                // We don't track magic wands resources
                HashSet::new()
            }

            vir::Expr::LetExpr(ref _variable, ref _expr, ref _body, _) => {
                unreachable!("Let expressions should be introduced after fold/unfold.");
            }

            vir::Expr::Downcast(ref base, ref _enum_place, ref _variant_field) => {
                base.get_footprint(predicates)
            }

            vir::Expr::SnapApp(ref expr, _) => expr.get_footprint(predicates),
        };
        trace!("get_footprint {} = {:?}", self, res);
        res
    }
}

pub trait PredicateFootprintGetter {
    /// Returns the footprint of the body of a predicate, that is the permissions that must be
    /// added/removed when executing a `fold/unfold pred` statement.
    /// When the method is called on a predicate encoding an enumeration and `maybe_variant` is
    /// `None` then the result is an under-approximation. The result is precise in all other cases.
    fn get_body_footprint(
        &self,
        maybe_variant: &vir::MaybeEnumVariantIndex,
    ) -> HashSet<Perm>;
}

impl PredicateFootprintGetter for vir::Predicate {
    fn get_body_footprint(
        &self,
        maybe_variant: &vir::MaybeEnumVariantIndex,
    ) -> HashSet<Perm> {
        match self {
            vir::Predicate::Struct(p) => {
                assert!(maybe_variant.is_none());
                p.get_body_footprint()
            }
            vir::Predicate::Enum(p) => {
                if let Some(variant) = maybe_variant {
                    p.get_body_footprint(variant)
                } else {
                    // This case happens when Prusti unfolds an enumeration without statically
                    // knowing the variant. For example, when encoding the lookup of a
                    // discriminant *before* entering a match expression.
                    // Under-approximating is better than over-approximating because it preserves
                    // the invariant that permissions in the fold-unfold state are an
                    // under-approximation of the permissions actually available in Viper.
                    p.get_underapproximated_body_footprint()
                }
            }
            vir::Predicate::Bodyless(_, _) => HashSet::new(),
            vir::Predicate::CreditUnit(_p) => HashSet::new(),       //TODO: ?
        }
    }
}

pub trait StructPredicateFootprintGetter {
    /// Returns the footprint of the body of a predicate encoding a structure, that is the
    /// permissions that must be added/removed when executing a `fold/unfold pred` statement.
    fn get_body_footprint(&self) -> HashSet<Perm>;
}

impl StructPredicateFootprintGetter for vir::StructPredicate {
    fn get_body_footprint(&self) -> HashSet<Perm> {
        match self.body {
            Some(ref body) => {
                // A predicate body should not contain unfolding expression
                let predicates = HashMap::new();
                body.get_footprint(&predicates)
            }
            None => HashSet::new(),
        }
    }
}

pub trait EnumPredicateFootprintGetter {
    /// Returns the footprint of the body of a predicate encoding an enumeration, that is the
    /// permissions that must be added/removed when executing a `fold/unfold pred` statement.
    fn get_body_footprint(&self, variant: &vir::EnumVariantIndex) -> HashSet<Perm>;

    /// Returns the footprint coontribution of a given variant of an enumeration.
    fn get_variant_footprint(&self, variant: &vir::EnumVariantIndex) -> HashSet<Perm>;

    /// Returns the intersection of `get_body_footprint` called on each possible variant.
    fn get_underapproximated_body_footprint(&self) -> HashSet<Perm>;
}

impl EnumPredicateFootprintGetter for vir::EnumPredicate {
    fn get_body_footprint(&self, variant: &vir::EnumVariantIndex) -> HashSet<Perm> {
        let mut perms = self.get_underapproximated_body_footprint();
        perms.extend(self.get_variant_footprint(variant));
        perms
    }

    fn get_variant_footprint(&self, variant: &vir::EnumVariantIndex) -> HashSet<Perm> {
        let mut perms = HashSet::new();
        let this: vir::Expr = self.this.clone().into();
        let variant_name = variant.get_variant_name();
        perms.insert(
            Perm::Acc(
                this.clone().variant(variant_name),
                PermAmount::Write,
            )
        );
        perms.insert(
            Perm::Pred(
                this.clone().variant(variant_name),
                PermAmount::Write,
            )
        );
        perms
    }

    fn get_underapproximated_body_footprint(&self) -> HashSet<Perm> {
        let mut perms = HashSet::new();
        perms.insert(
            Perm::Acc(
                vir::Expr::from(self.this.clone()).field(self.discriminant_field.clone()),
                PermAmount::Write,
            )
        );
        perms
    }
}
