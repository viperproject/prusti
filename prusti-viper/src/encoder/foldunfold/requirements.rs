// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::places_utils::{union, union3};
use crate::encoder::foldunfold::perm::Perm::*;
use crate::encoder::foldunfold::perm::*;
use crate::encoder::foldunfold::footprint::*;
use prusti_common::vir;
use prusti_common::vir::PermAmount;
use std::collections::HashMap;
use std::collections::HashSet;
use std::iter::FromIterator;
use log::{trace, debug};

pub trait RequiredPermissionsGetter {
    /// Returns the permissions required for the expression/statement to be well-defined.
    /// The result might be an over-approximation, as in the `vir::Expr::FuncApp` case.
    fn get_required_permissions(
        &self,
        predicates: &HashMap<String, vir::Predicate>,
        old_exprs: &HashMap<String, Vec<vir::Expr>>,
    ) -> HashSet<Perm>;
}

impl<'a, A: RequiredPermissionsGetter> RequiredPermissionsGetter for &'a A {
    fn get_required_permissions(
        &self,
        predicates: &HashMap<String, vir::Predicate>,
        old_exprs: &HashMap<String, Vec<vir::Expr>>,
    ) -> HashSet<Perm> {
        (*self).get_required_permissions(predicates, old_exprs)
    }
}

impl<'a, A: RequiredPermissionsGetter> RequiredPermissionsGetter for Vec<A> {
    fn get_required_permissions(
        &self,
        predicates: &HashMap<String, vir::Predicate>,
        old_exprs: &HashMap<String, Vec<vir::Expr>>,
    ) -> HashSet<Perm> {
        self.iter().fold(HashSet::new(), |res, x| {
            res.union(&x.get_required_permissions(predicates, old_exprs))
                .cloned()
                .collect()
        })
    }
}

impl RequiredPermissionsGetter for vir::Stmt {
    fn get_required_permissions(
        &self,
        predicates: &HashMap<String, vir::Predicate>,
        old_exprs: &HashMap<String, Vec<vir::Expr>>,
    ) -> HashSet<Perm> {
        match self {
            &vir::Stmt::Comment(_) => HashSet::new(),

            &vir::Stmt::Label(ref label) => {
                // A label has to ensure that all usages of labelled-old expressions can be
                // solved by unfolding, and not folding, permissions.
                if let Some(exprs) = old_exprs.get(label) {
                    exprs.get_required_permissions(predicates, old_exprs)
                } else {
                    HashSet::new()
                }
            },

            &vir::Stmt::Inhale(ref expr) => {
                // footprint = used - inhaled
                perm_difference(
                    expr.get_required_permissions(predicates, old_exprs),
                    expr.get_footprint(predicates),
                )
            }

            &vir::Stmt::Exhale(ref expr, ref pos)
            | &vir::Stmt::Assert(ref expr, ref pos)
            | &vir::Stmt::Obtain(ref expr, ref pos) => {
                let perms = expr.get_required_permissions(predicates, old_exprs);
                perms
                    .into_iter()
                    .map(|perm| perm.set_default_pos(*pos))
                    .collect()
            }

            &vir::Stmt::MethodCall(_, ref args, ref vars) => {
                // Preconditions and postconditions are empty
                assert!(args.is_empty());
                HashSet::from_iter(
                    vars.iter()
                        .cloned()
                        .map(|v| Acc(vir::Expr::local(v), PermAmount::Write)),
                )
            }

            &vir::Stmt::Assign(ref lhs, ref rhs, _kind) => {
                let mut res = rhs.get_required_permissions(predicates, old_exprs);
                res.insert(Acc(lhs.clone(), PermAmount::Write));
                res
            }

            &vir::Stmt::Fold(_, ref args, perm_amount, ref variant, _) => {
                assert_eq!(args.len(), 1);
                let place = &args[0];
                debug_assert!(place.is_place());

                // We want to temporarily unfold place
                let predicate_name = place.typed_ref_name().unwrap();
                let predicate = predicates.get(&predicate_name).unwrap();

                let pred_self_place: vir::Expr = predicate.self_place();
                let places_in_pred: HashSet<Perm> = predicate
                    .get_body_footprint(variant)
                    .into_iter()
                    .map(|perm| {
                        perm.map_place(|p| p.replace_place(&pred_self_place, &place))
                            .init_perm_amount(perm_amount)
                    })
                    .collect();

                places_in_pred
            }

            &vir::Stmt::Unfold(ref _pred_name, ref args, perm_amount, ref _variant) => {
                assert_eq!(args.len(), 1);
                let place = &args[0];
                debug_assert!(place.is_place());
                place
                    .get_required_permissions(predicates, old_exprs)
                    .into_iter()
                    .map(|perm| perm.init_perm_amount(perm_amount))
                    .collect()
            }

            &vir::Stmt::BeginFrame | &vir::Stmt::EndFrame => HashSet::new(),

            &vir::Stmt::TransferPerm(ref lhs, _, unchecked) => {
                let mut res = HashSet::new();
                if !unchecked {
                    res.insert(Acc(lhs.clone(), PermAmount::Read));
                }
                res
            }

            &vir::Stmt::PackageMagicWand(
                vir::Expr::MagicWand(ref _lhs, ref _rhs, ref _borrow, ref _pos),
                ref _package_stmts,
                ref _label,
                ref _vars,
                ref _position,
            ) => {
                // We model the magic wand as "assert lhs; stmts; exhale rhs"
                HashSet::new()
            }

            &vir::Stmt::ApplyMagicWand(
                vir::Expr::MagicWand(ref lhs, ref _rhs, ref _borrow, ref _wand_pos),
                ref _apply_pos,
            ) => {
                // We model the magic wand as "assert lhs; inhale rhs"
                lhs.get_required_permissions(predicates, old_exprs)
            }

            &vir::Stmt::ExpireBorrows(ref _dag) => {
                HashSet::new() // TODO: #133
            }

            &vir::Stmt::If(ref guard, ref then, ref elze) => {
                let guard_reqs = guard.get_required_permissions(predicates, old_exprs);
                let then_reqs = then.get_required_permissions(predicates, old_exprs);
                let elze_reqs = elze.get_required_permissions(predicates, old_exprs);
                let then_else_reqs = then_reqs.intersection(&elze_reqs).cloned().collect();
                guard_reqs.union(&then_else_reqs).cloned().collect()
            }

            &vir::Stmt::Downcast(ref enum_place, ref variant_field) => {
                // Delegate
                vir::Expr::downcast(true.into(), enum_place.clone(), variant_field.clone())
                    .get_required_permissions(predicates, old_exprs)
            }

            ref x => unimplemented!("{}", x),
        }
    }
}

impl RequiredPermissionsGetter for vir::Expr {
    fn get_required_permissions(
        &self,
        predicates: &HashMap<String, vir::Predicate>,
        old_exprs: &HashMap<String, Vec<vir::Expr>>,
    ) -> HashSet<Perm> {
        trace!("[enter] get_required_permissions(expr={})", self);
        let permissions = match self {
            vir::Expr::Const(_, _) => HashSet::new(),

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
                            .update_perm_amount(*perm_amount)
                    })
                    .collect();

                // Simulate temporary unfolding of `place`
                let expr_req_places = expr.get_required_permissions(predicates, old_exprs);
                let mut req_places: HashSet<_> = perm_difference(expr_req_places, places_in_pred);
                req_places.insert(Pred(place.clone(), *perm_amount));
                req_places.into_iter().collect()
            }

            vir::Expr::LabelledOld(_label, _expr, _) => HashSet::new(),

            vir::Expr::PredicateAccessPredicate(_, box place, _perm_amount, _) => {
                debug_assert!(place.is_place());
                let epsilon = PermAmount::Read;
                let result = match place.get_label() {
                    None => {
                        if place.is_old() {
                            vec![Pred(place.clone(), epsilon)].into_iter().collect()
                        } else {
                            vec![Pred(place.clone(), epsilon), Acc(place.clone(), epsilon)]
                                .into_iter()
                                .collect()
                        }
                    }
                    Some(label) => {
                        if place.is_old() {
                            vec![Pred(place.clone().old(label), epsilon)]
                                .into_iter()
                                .collect()
                        } else {
                            vec![
                                Pred(place.clone().old(label), epsilon),
                                Acc(place.clone().old(label), epsilon),
                            ]
                            .into_iter()
                            .collect()
                        }
                    }
                };
                result
            }

            vir::Expr::CreditAccessPredicate(_, _args, _frac_perm_amount, _) => HashSet::new(),     //TODO ?

            vir::Expr::FieldAccessPredicate(expr, _perm_amount, _) => expr
                .get_required_permissions(predicates, old_exprs)
                .into_iter()
                .collect(),

            vir::Expr::UnaryOp(_, expr, _) => expr.get_required_permissions(predicates, old_exprs),

            vir::Expr::BinOp(_, box left, box right, _) => {
                vec![left, right].get_required_permissions(predicates, old_exprs)
            }

            vir::Expr::ContainerOp(_, box left, box right, _) => {
                vec![left, right].get_required_permissions(predicates, old_exprs)
            }

            vir::Expr::Seq(_, elems, _) => {
                elems.get_required_permissions(predicates, old_exprs)
            }

            vir::Expr::Cond(box guard, box then, box elze, _) => {
                vec![guard, then, elze].get_required_permissions(predicates, old_exprs)
            }

            vir::Expr::LetExpr(_variable, _expr, _body, _) => {
                unreachable!("Let expressions should be introduced after fold/unfold.");
            }

            vir::Expr::ForAll(vars, _triggers, box body, _) => {
                assert!(vars.iter().all(|var| !var.typ.is_ref()));

                let vars_places: HashSet<_> = vars
                    .iter()
                    .map(|var| Acc(vir::Expr::local(var.clone()), PermAmount::Write))
                    .collect();
                perm_difference(body.get_required_permissions(predicates, old_exprs), vars_places)
            }

            vir::Expr::Local(..) => HashSet::new(),

            vir::Expr::AddrOf(..) => unreachable!(),

            vir::Expr::Variant(..) => Some(Acc(self.clone(), PermAmount::Read))
                .into_iter()
                .collect(),

            vir::Expr::Field(..) => Some(Acc(self.clone(), PermAmount::Read))
                .into_iter()
                .collect(),

            vir::Expr::MagicWand(ref _lhs, ref _rhs, ref _borrow, _) => {
                // Not exactly Viper's semantics
                HashSet::new()
            }

            vir::Expr::FuncApp(_, ref args, ..) |
            vir::Expr::DomainFuncApp(_, ref args, _)=> {
                args.iter()
                    .map(|arg| {
                        if arg.is_place() && arg.get_type().is_ref() {
                            // FIXME: A hack: have unfolded Rust references in the precondition to
                            // simplify our life. A proper solution would be to look up the
                            // real function precondition.
                            if let Some(field_place) = arg.try_deref() {
                                vir::Expr::and(
                                    vir::Expr::acc_permission(
                                        field_place.clone(),
                                        vir::PermAmount::Read,
                                    ),
                                    vir::Expr::pred_permission(field_place, vir::PermAmount::Read)
                                        .unwrap(),
                                )
                            } else {
                                let predicate_name = arg.get_type().name();
                                vir::Expr::predicate_access_predicate(
                                    predicate_name.clone(),
                                    arg.clone().into(),
                                    PermAmount::Read,
                                )
                            }
                        } else {
                            debug!("arg {} is not a place with type ref", arg);
                            arg.clone()
                        }
                    })
                    .collect::<Vec<_>>()
                    .get_required_permissions(predicates, old_exprs)
            }

            vir::Expr::InhaleExhale(..) => HashSet::new(),

            vir::Expr::Downcast(_, ref enum_place, _) => {
                let predicate_name = enum_place.typed_ref_name().unwrap();
                let predicate = predicates.get(&predicate_name).unwrap();
                if let vir::Predicate::Enum(enum_predicate) = predicate {
                    // We want to have the enum unfolded
                    enum_place.clone()
                        .field(enum_predicate.discriminant_field.clone())
                        .get_required_permissions(predicates, old_exprs)
                } else {
                    unreachable!()
                }
            }

            vir::Expr::SnapApp(..) => {
                unreachable!("Snapshots should be patched before fold/unfold. {:?}", self);
            }
        };
        trace!(
            "[exit] get_required_permissions(expr={}): {:#?}",
            self,
            permissions
        );
        permissions
    }
}
