// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! An optimization that pulls `unfolding` expressions as much up as
//! possible in this way hoping to reduce the number of unfolds. We
//! cannot pull unfolding if:
//!
//! 1.  There is a conflicting folding requirement coming from a
//!     function application.
//! 2.  There is an implication that branches on a enum discriminant.


use super::super::super::{ast, borrows, cfg};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::mem;


pub trait FoldingOptimizer {
    fn optimize(self) -> Self;
}

impl FoldingOptimizer for cfg::CfgMethod {
    fn optimize(mut self) -> Self {
        let mut sentinel_stmt = ast::Stmt::Comment(String::from("moved out stmt"));
        let mut optimizer = StmtOptimizer {};
        for block in &mut self.basic_blocks {
            for stmt in &mut block.stmts {
                mem::swap(&mut sentinel_stmt, stmt);
                sentinel_stmt = ast::StmtFolder::fold(&mut optimizer, sentinel_stmt);
                mem::swap(&mut sentinel_stmt, stmt);
            }
        }
        self
    }
}

impl FoldingOptimizer for ast::Function {
    fn optimize(mut self) -> Self {
        trace!("[enter] FoldingOptimizer function_name={}", self.name);
        self.body = self.body.map(|e| e.optimize());
        trace!("[exit] FoldingOptimizer function_name={}", self.name);
        self
    }
}

impl FoldingOptimizer for ast::Expr {
    fn optimize(self) -> Self {
        trace!("[enter] FoldingOptimizer::optimize = \n{}", self);
        let mut optimizer = ExprOptimizer {
            unfoldings: HashMap::new(),
            requirements: HashSet::new(),
        };
        let new_expr = ast::ExprFolder::fold(&mut optimizer, self);
        let r = restore_unfoldings(optimizer.get_unfoldings(), new_expr);
        trace!("[exit] FoldingOptimizer::optimize = \n{}", r);
        r
    }
}

struct StmtOptimizer {
}

impl ast::StmtFolder for StmtOptimizer {
    fn fold_inhale(&mut self, expr: ast::Expr, folding: ast::FoldingBehaviour) -> ast::Stmt {
        let new_expr = if folding == ast::FoldingBehaviour::Expr {
            expr.optimize()
        } else {
            expr
        };
        ast::Stmt::Inhale(new_expr, folding)
    }
}

type UnfoldingMap = HashMap<ast::Expr, (String, ast::PermAmount, ast::MaybeEnumVariantIndex)>;
type RequirementSet = HashSet<ast::Expr>;

struct ExprOptimizer {
    /// Predicate argument → (predicate name, amount, enum index).
    unfoldings: UnfoldingMap,
    /// Unfolding requirements: how deeply a specific place should be unfolded.
    requirements: RequirementSet,
}

impl ExprOptimizer {
    fn get_unfoldings(&mut self) -> UnfoldingMap {
        mem::replace(&mut self.unfoldings, HashMap::new())
    }
    fn get_requirements(&mut self) -> RequirementSet {
        mem::replace(&mut self.requirements, HashSet::new())
    }
}

fn restore_unfoldings_boxed(unfolding_map: UnfoldingMap, expr: Box<ast::Expr>) -> Box<ast::Expr> {
    box restore_unfoldings(unfolding_map, *expr)
}

/// Restore unfoldings on a given expression.
fn restore_unfoldings(unfolding_map: UnfoldingMap, mut expr: ast::Expr) -> ast::Expr {
    let mut unfoldings: Vec<_> = unfolding_map.into_iter().collect();
    unfoldings.sort_by(|(k1, _), (k2, _)| {
        if k1 == k2 {
            Ordering::Equal
        } else {
            let base_k1 = k1.get_base().name;
            let base_k2 = k2.get_base().name;
            if base_k1 < base_k2 {
                Ordering::Less
            } else if base_k1 > base_k2 {
                Ordering::Greater
            } else {
                if k2.has_prefix(k1) {
                    Ordering::Greater
                } else if k1.has_prefix(k2) {
                    Ordering::Less
                } else {
                    format!("{}", k1).cmp(&format!("{}", k2))
                }
            }
        }
    });
    for (arg, (name, perm_amount, variant)) in unfoldings {
        let position = expr.pos();
        expr = ast::Expr::Unfolding(
            name,
            vec![arg],
            box expr,
            perm_amount,
            variant,
            position,
        );
    }
    expr
}

/// Check whether the requirements are conflicting.
///
/// Returns a set of conflicting bases. The empty set means no conflicts.
fn check_requirements_conflict(
    reqs1: &RequirementSet,
    reqs2: &RequirementSet
) -> HashSet<ast::Expr> {
    let mut conflict_set = HashSet::new();
    for place1 in reqs1 {
        for place2 in reqs2 {
            // Check if we require the same place to be unfolded at a different depth.
            let (base1, components1) = place1.explode_place();
            let (base2, components2) = place2.explode_place();
            if place1.has_proper_prefix(place2) && !reqs1.contains(place2) {
                debug!("{} has_proper_prefix {}", place1, place2);
                conflict_set.insert(base2);
            } else if place2.has_proper_prefix(place1) && !reqs2.contains(place1) {
                debug!("{} has_proper_prefix {}", place2, place1);
                conflict_set.insert(base1);
            } else if base1 == base2 && !place1.has_prefix(place2) && !place2.has_prefix(place1) {
                // Check if we have different variants.
                let mut len1 = components1.len();
                let mut len2 = components2.len();
                for (part1, part2) in components1.into_iter().zip(components2.into_iter()) {
                    len1 -= 1;
                    len2 -= 1;
                    if part1 != part2 {
                        match (part1, part2) {
                            (ast::PlaceComponent::Variant(..),
                             ast::PlaceComponent::Variant(..)) => {
                                if len1 != 0 || len2 != 0 {
                                    debug!("different variants: {} {}", place1, place2);
                                    // If variant is the last component of the place, then we are
                                    // still fine because we will try to unfold under implication.
                                    conflict_set.insert(base1);
                                }
                            }
                            (ast::PlaceComponent::Field(ast::Field { name, .. }, _),
                             ast::PlaceComponent::Variant(..)) |
                            (ast::PlaceComponent::Variant(..),
                             ast::PlaceComponent::Field(ast::Field { name, .. }, _)) => {
                                if name == "discriminant" {
                                    debug!("guarded permission: {} {}", place1, place2);
                                    // If we are checking discriminant, this means that the
                                    // permission is guarded.
                                    if len1 != 0 || len2 != 0 {
                                        // However, if the variant is the last component of the
                                        // place, then we are still fine because we will try to
                                        // unfold under implication.
                                        conflict_set.insert(base1);
                                    }
                                }
                            }
                            _ => {}
                        }
                        break;
                    }
                }
            }
        }
    }
    conflict_set
}

/// Split the unfoldings map into two: to restore and to keep.
fn split_unfoldings(
    unfoldings: UnfoldingMap,
    conflicts: &HashSet<ast::Expr>
) -> (UnfoldingMap, UnfoldingMap) {
    let mut to_restore = HashMap::new();
    let mut to_keep = HashMap::new();
    for (place, data) in unfoldings {
        if conflicts.iter().any(|c| place.has_prefix(c)) {
            to_restore.insert(place, data);
        } else {
            to_keep.insert(place, data);
        }
    }
    (to_restore, to_keep)
}

/// Find unfoldings that are in both sets.
fn find_common_unfoldings2(
    first: UnfoldingMap,
    mut second: UnfoldingMap,
) -> (UnfoldingMap, UnfoldingMap, UnfoldingMap) {
    let mut common = HashMap::new();
    let mut new_first = HashMap::new();
    for (place, data) in first {
        if second.contains_key(&place) {
            second.remove(&place);
            common.insert(place, data);
        } else {
            new_first.insert(place, data);
        }
    }
    (common, new_first, second)
}

/// Find unfoldings that are in all three sets.
fn find_common_unfoldings3<'a>(
    mut first: UnfoldingMap,
    mut first_reqs: &'a RequirementSet,
    mut second: UnfoldingMap,
    mut second_reqs: &'a RequirementSet,
    mut third: UnfoldingMap,
    third_reqs: &'a RequirementSet,
) -> (UnfoldingMap, UnfoldingMap, UnfoldingMap, UnfoldingMap) {
    let mut common = HashMap::new();
    let mut new_first = HashMap::new();
    let swap = first.is_empty();
    if swap {
        mem::swap(&mut first, &mut second);
        mem::swap(&mut first_reqs, &mut second_reqs);
    }
    for (place, data) in first {
        let second_agrees = second.contains_key(&place) ||
            second_reqs.iter().all(|p| !p.has_prefix(&place));
        let third_agrees = third.contains_key(&place) ||
            third_reqs.iter().all(|p| !p.has_prefix(&place));
        if second_agrees && third_agrees {
            second.remove(&place);
            third.remove(&place);
            common.insert(place, data);
        } else {
            new_first.insert(place, data);
        }
    }
    if swap {
        (common, second, new_first, third)
    } else {
        (common, new_first, second, third)
    }
}

fn update_requirements(requirements: &mut RequirementSet, mut new_requirements: Vec<ast::Expr>) {
    new_requirements.sort_by_cached_key(|place| -(place.place_depth() as i32));
    for place in new_requirements {
        requirements.retain(|p| !p.has_prefix(&place));
        requirements.insert(place);
    }
}

fn merge_requirements_and_unfoldings2(
    first: Box<ast::Expr>,
    mut first_unfoldings: UnfoldingMap,
    mut first_requirements: RequirementSet,
    second: Box<ast::Expr>,
    second_unfoldings: UnfoldingMap,
    second_requirements: RequirementSet,
) -> (RequirementSet, UnfoldingMap, Box<ast::Expr>, Box<ast::Expr>) {

    trace!("[enter] merge_requirements_and_unfoldings");
    use utils::to_string::ToString;
    trace!("reqs: {}", first_requirements.iter().to_sorted_multiline_string());
    trace!("unfoldings: {}", first_unfoldings.keys().to_sorted_multiline_string());
    trace!("reqs: {}", second_requirements.iter().to_sorted_multiline_string());
    trace!("unfoldings: {}", second_unfoldings.keys().to_sorted_multiline_string());

    let conflicts = check_requirements_conflict(&first_requirements, &second_requirements);
    trace!("conflicts: {}", conflicts.iter().to_sorted_multiline_string());

    if conflicts.is_empty() {
        first_requirements.extend(second_requirements);
        first_unfoldings.extend(second_unfoldings);
        (first_requirements, first_unfoldings, first, second)
    } else {

        let (common, first_unfoldings, second_unfoldings) = find_common_unfoldings2(
            first_unfoldings, second_unfoldings);

        let (first_to_restore, first_to_keep) = split_unfoldings(
            first_unfoldings, &conflicts);
        let (second_to_restore, second_to_keep) = split_unfoldings(
            second_unfoldings, &conflicts);

        let mut new_requirements = first_requirements;
        new_requirements.extend(second_requirements);
        update_requirements(&mut new_requirements, first_to_restore.keys().cloned().collect());
        update_requirements(&mut new_requirements, second_to_restore.keys().cloned().collect());

        let first_restored = restore_unfoldings_boxed(first_to_restore, first);
        let second_restored = restore_unfoldings_boxed(second_to_restore, second);

        let mut new_unfoldings = common;
        new_unfoldings.extend(first_to_keep);
        new_unfoldings.extend(second_to_keep);

        (new_requirements, new_unfoldings, first_restored, second_restored)
    }
}

impl ast::ExprFolder for ExprOptimizer {

    fn fold(&mut self, expr: ast::Expr) -> ast::Expr {
        match expr {
            ast::Expr::LabelledOld(..) => {
                if expr.is_place() {
                    self.requirements.insert(expr.clone());
                }
                expr
            },
            ast::Expr::Unfolding(name, mut args, box body, perm, variant, _) => {
                assert!(args.len() == 1);
                let new_expr = self.fold(body);
                self.unfoldings.insert(args.pop().unwrap(), (name, perm, variant));
                new_expr
            }
            _ => {
                if expr.is_place() {
                    self.requirements.insert(expr.clone());
                    expr
                } else {
                    ast::default_fold_expr(self, expr)
                }
            }
        }
    }
    fn fold_unfolding(
        &mut self,
        _name: String,
        _args: Vec<ast::Expr>,
        _expr: Box<ast::Expr>,
        _perm: ast::PermAmount,
        _variant: ast::MaybeEnumVariantIndex,
        _pos: ast::Position,
    ) -> ast::Expr {
        unreachable!();
    }
    fn fold_labelled_old(
        &mut self,
        _label: String,
        _body: Box<ast::Expr>,
        _pos: ast::Position
    ) -> ast::Expr {
        unreachable!();
    }
    fn fold_magic_wand(
        &mut self,
        lhs: Box<ast::Expr>,
        rhs: Box<ast::Expr>,
        borrow: Option<borrows::Borrow>,
        pos: ast::Position,
    ) -> ast::Expr {
        ast::Expr::MagicWand(lhs, rhs, borrow, pos)
    }
    fn fold_predicate_access_predicate(
        &mut self,
        _name: String,
        _arg: Box<ast::Expr>,
        _perm_amount: ast::PermAmount,
        _pos: ast::Position,
    ) -> ast::Expr {
        unimplemented!()
    }
    fn fold_field_access_predicate(
        &mut self,
        _receiver: Box<ast::Expr>,
        _perm_amount: ast::PermAmount,
        _pos: ast::Position
    ) -> ast::Expr {
        unimplemented!()
    }
    fn fold_bin_op(
        &mut self,
        kind: ast::BinOpKind,
        first: Box<ast::Expr>,
        second: Box<ast::Expr>,
        pos: ast::Position
    ) -> ast::Expr {
        let f = first.clone();
        let s = second.clone();
        let first_folded = self.fold_boxed(first);
        let first_unfoldings = self.get_unfoldings();
        let first_requirements = self.get_requirements();

        let second_folded = self.fold_boxed(second);
        let second_unfoldings = self.get_unfoldings();
        let second_requirements = self.get_requirements();

        trace!("fold_bin_op: {} {} {}", kind, f, s);

        let (new_reqs, new_unfoldings, new_first, new_second) = merge_requirements_and_unfoldings2(
            first_folded, first_unfoldings, first_requirements,
            second_folded, second_unfoldings, second_requirements);

        self.requirements = new_reqs;
        self.unfoldings = new_unfoldings;
        ast::Expr::BinOp(kind, new_first, new_second, pos)
    }
    fn fold_cond(
        &mut self,
        guard: Box<ast::Expr>,
        then_expr: Box<ast::Expr>,
        else_expr: Box<ast::Expr>,
        pos: ast::Position
    ) -> ast::Expr {
        let g = guard.clone();
        let f = then_expr.clone();
        let s = else_expr.clone();
        let guard_folded = self.fold_boxed(guard);
        let guard_unfoldings = self.get_unfoldings();
        let guard_requirements = self.get_requirements();

        let then_folded = self.fold_boxed(then_expr);
        let then_unfoldings = self.get_unfoldings();
        let then_requirements = self.get_requirements();

        let else_folded = self.fold_boxed(else_expr);
        let else_unfoldings = self.get_unfoldings();
        let else_requirements = self.get_requirements();

        trace!("\n\nfold_cond:\ng = {}\nt = {}\ne = {}", g, f, s);

        let mut conflicts = check_requirements_conflict(&guard_requirements, &then_requirements);
        conflicts.extend(check_requirements_conflict(&guard_requirements, &else_requirements));
        conflicts.extend(check_requirements_conflict(&then_requirements, &else_requirements));

        if conflicts.is_empty() {

            self.requirements = guard_requirements;
            self.requirements.extend(then_requirements);
            self.requirements.extend(else_requirements);

            self.unfoldings = guard_unfoldings;
            self.unfoldings.extend(then_unfoldings);
            self.unfoldings.extend(else_unfoldings);

            ast::Expr::Cond(
                guard_folded,
                then_folded,
                else_folded,
                pos,
            )
        } else {
            let (common, guard_unfoldings, then_unfoldings, else_unfoldings
                 ) = find_common_unfoldings3(
                guard_unfoldings, &guard_requirements,
                then_unfoldings, &then_requirements,
                else_unfoldings, &else_requirements);

            let (guard_to_restore, guard_to_keep) = split_unfoldings(
                guard_unfoldings, &conflicts);
            let (then_to_restore, then_to_keep) = split_unfoldings(
                then_unfoldings, &conflicts);
            let (else_to_restore, else_to_keep) = split_unfoldings(
                else_unfoldings, &conflicts);

            self.requirements = guard_requirements;
            self.requirements.extend(then_requirements);
            self.requirements.extend(else_requirements);
            update_requirements(&mut self.requirements, guard_to_restore.keys().cloned().collect());
            update_requirements(&mut self.requirements, then_to_restore.keys().cloned().collect());
            update_requirements(&mut self.requirements, else_to_restore.keys().cloned().collect());

            let guard_restored = restore_unfoldings_boxed(guard_to_restore, guard_folded);
            let then_restored = restore_unfoldings_boxed(then_to_restore, then_folded);
            let else_restored = restore_unfoldings_boxed(else_to_restore, else_folded);

            self.unfoldings = common;
            self.unfoldings.extend(guard_to_keep);
            self.unfoldings.extend(then_to_keep);
            self.unfoldings.extend(else_to_keep);

            ast::Expr::Cond(
                guard_restored,
                then_restored,
                else_restored,
                pos,
            )
        }
    }
    fn fold_let_expr(
        &mut self,
        _var: ast::LocalVar,
        _expr: Box<ast::Expr>,
        _body: Box<ast::Expr>,
        _pos: ast::Position
    ) -> ast::Expr {
        unreachable!();
    }
}
