// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::vir::{
    ast::*,
    cfg,
    cfg::CfgMethod,
    utils::{walk_functions, walk_methods},
    CfgBlock,
};
use std::collections::{BTreeMap, BTreeSet};

fn collect_info_from_methods_and_functions(
    methods: &[CfgMethod],
    functions: &[Function],
) -> UsedPredicateCollector {
    let mut collector = UsedPredicateCollector::new();
    walk_methods(methods, &mut collector);
    walk_functions(functions, &mut collector);

    // DeadBorrowToken$ is a used predicate but it does not appear in VIR
    // becaue it is only created when viper code is created from VIR
    collector
        .used_predicates
        .insert("DeadBorrowToken$".to_string());
    collector
}

/// Computes a map of Predicate to the predicates used  in that predicate
fn get_used_predicates_in_predicate_map(
    predicates: &[Predicate],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut collector = UsedPredicateCollector::new();
    let mut map = BTreeMap::new();

    for pred in predicates {
        match pred {
            Predicate::Struct(StructPredicate {
                name,
                body: Some(e),
                ..
            }) => {
                ExprWalker::walk(&mut collector, e);
                let mut res = BTreeSet::new();
                std::mem::swap(&mut res, &mut collector.used_predicates);
                map.insert(name.clone(), res);
            }
            Predicate::Struct(StructPredicate { body: None, .. }) => { /* ignore */ }
            Predicate::Enum(p) => {
                let discriminant_loc = Expr::from(p.this.clone()).field(p.discriminant_field.clone());
                ExprWalker::walk(&mut collector, &discriminant_loc);
                ExprWalker::walk(&mut collector, &p.discriminant_bounds);

                for (e, _, sp) in &p.variants {
                    ExprWalker::walk(&mut collector, e);
                    collector.used_predicates.insert(sp.name.to_string());
                    sp.body
                        .iter()
                        .for_each(|e| ExprWalker::walk(&mut collector, e))
                }

                let mut res = BTreeSet::new();
                std::mem::swap(&mut res, &mut collector.used_predicates);
                map.insert(p.name.clone(), res);
            }
            Predicate::Bodyless(_, _) => { /* ignore */ }
            Predicate::CreditUnit(CreditUnitPredicate {
                name,
                body: Some(e),
                ..
            }) => {
                ExprWalker::walk(&mut collector, e);
                let mut res = BTreeSet::new();
                std::mem::swap(&mut res, &mut collector.used_predicates);
                map.insert(name.clone(), res);
            }
            Predicate::CreditUnit(CreditUnitPredicate { body: None, .. }) => { /* ignore */ }
        }
    }
    return map;
}

/// Return the set of the predicates in the predicate map that are actually
/// reachable from the provided set of predicates
fn compute_reachable_predicates(
    predicates_in_predicates: &BTreeMap<String, BTreeSet<String>>,
    used_predicates: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut visited = BTreeSet::new();
    for to_visit in used_predicates {
        visit_predicate(to_visit, predicates_in_predicates, &mut visited);
    }

    visited
}

/// Function used by compute_reachable_predicates to visit all the predicates
fn visit_predicate(
    to_visit: &str,
    predicates_in_predicates: &BTreeMap<String, BTreeSet<String>>,
    visited: &mut BTreeSet<String>,
) {
    if visited.contains(to_visit) {
        return;
    }

    visited.insert(to_visit.to_string());

    if let Some(v) = predicates_in_predicates.get(to_visit) {
        for p in v {
            visit_predicate(p, predicates_in_predicates, visited)
        }
    }
}

/// Delete all unused predicates and eliminate bodies of predicates that are never folded or unfolded
pub fn delete_unused_predicates(
    methods: &[CfgMethod],
    functions: &[Function],
    mut predicates: Vec<Predicate>,
) -> Vec<Predicate> {
    let collector = collect_info_from_methods_and_functions(methods, functions);
    let used_predicates_in_functions_and_methods = collector.used_predicates;
    let folded_predicates = collector.folded_predicates;
    let mut predicates_in_predicates_map = get_used_predicates_in_predicate_map(&predicates);

    debug!(
        "The used predicates in functions and methods are {:?}",
        &used_predicates_in_functions_and_methods
    );

    // Remove the bodies of predicats that are never folded or unfolded
    predicates.iter_mut().for_each(|predicate| {
        let predicate_name = predicate.name().to_string();
        if !folded_predicates.contains(predicate.name()) {
            if let Predicate::Struct(sp) = predicate {
                debug!("Removed body of {}", predicate_name);
                sp.body = None;

                // since the predicate now has no body update the predicate map accordingly
                predicates_in_predicates_map.remove(&predicate_name);
            }
        }
    });

    debug!(
        "Map of predicates used in predicates {:?}",
        &predicates_in_predicates_map
    );

    let reachable_predicates = compute_reachable_predicates(
        &predicates_in_predicates_map,
        &used_predicates_in_functions_and_methods,
    );

    debug!("All the used predicates are {:?}", &reachable_predicates);

    predicates.retain(|p| reachable_predicates.contains(p.name()));

    predicates
}

struct UsedPredicateCollector {
    /// set of all predicates that are used
    used_predicates: BTreeSet<String>,
    /// set of all predicates that are folded or unfolded
    folded_predicates: BTreeSet<String>,
}

impl UsedPredicateCollector {
    fn new() -> Self {
        UsedPredicateCollector {
            used_predicates: BTreeSet::new(),
            folded_predicates: BTreeSet::new(),
        }
    }
}

impl ExprWalker for UsedPredicateCollector {
    fn walk_predicate_access_predicate(
        &mut self,
        name: &str,
        arg: &Expr,
        _perm_amount: PermAmount,
        _pos: &Position,
    ) {
        self.used_predicates.insert(name.to_string());
        ExprWalker::walk(self, arg);
    }

    fn walk_credit_access_predicate(
        &mut self,
        name: &str,
        args: &Vec<Expr>,
        _frac_perm_amount: &FracPermAmount,
        _pos: &Position
    ) {
        self.used_predicates.insert(name.to_string());
        for arg in args {
            ExprWalker::walk(self, arg);
        }
    }

    fn walk_unfolding(
        &mut self,
        name: &str,
        args: &Vec<Expr>,
        body: &Expr,
        _perm: PermAmount,
        _variant: &MaybeEnumVariantIndex,
        _pos: &Position,
    ) {
        self.used_predicates.insert(name.to_string());
        self.folded_predicates.insert(name.to_string());

        for arg in args {
            ExprWalker::walk(self, arg);
        }
        ExprWalker::walk(self, body);
    }
}

impl StmtWalker for UsedPredicateCollector {
    fn walk_expr(&mut self, expr: &Expr) {
        ExprWalker::walk(self, expr);
    }
    fn walk_fold(
        &mut self,
        predicate_name: &str,
        _args: &Vec<Expr>,
        _perm: &PermAmount,
        _variant: &MaybeEnumVariantIndex,
        _pos: &Position,
    ) {
        self.folded_predicates.insert(predicate_name.to_string());
        self.used_predicates.insert(predicate_name.to_string());
    }

    fn walk_unfold(
        &mut self,
        predicate_name: &str,
        _args: &Vec<Expr>,
        _perm: &PermAmount,
        _variant: &MaybeEnumVariantIndex,
    ) {
        self.folded_predicates.insert(predicate_name.to_string());
        self.used_predicates.insert(predicate_name.to_string());
    }
}
