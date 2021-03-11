// This file should be in `prusti-common/src/vir/optimizations/purification/`,
// but it depends on encoder…

use prusti_common::vir::{self, ExprWalker, ExprFolder, StmtWalker, StmtFolder};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use log::{debug, trace};
use crate::encoder::Encoder;
use crate::encoder::snapshot_encoder::Snapshot;

/// Replaces shared references to pure Viper variables.
pub fn purify_shared_borrows(
    encoder: &Encoder,
    method: &mut vir::CfgMethod
) {
    purify_method(encoder, method);
}

fn purify_method(encoder: &Encoder, method: &mut vir::CfgMethod) {
    // A set of candidate references to be purified.
    let mut candidates = HashSet::new();
    debug!("method: {}", method.name());
    for var in &method.local_vars {
        match &var.typ {
            &vir::Type::TypedRef(ref typ) if typ.starts_with("ref$") => {
                trace!("  candidate: {}: {}", var.name, var.typ);
                candidates.insert(var.name.clone());
            }
            _ => {}
        };
    }
    if candidates.is_empty() {
        return;
    }
    // Collect variables that are dereferenced.
    let mut collector = VarDependencyCollector::default();
    vir::utils::walk_method(method, &mut collector);
    debug!(
        "VarDependencyCollector for method {} after collection {:?}",
        method.name(),
        collector
    );
    collector.compute_dereferenced_variables_fixpoint();
    debug!(
        "Dereferenced variables for method {} after fixpoint {:?}",
        method.name(),
        collector.dereferenced_variables
    );
    collector.compute_borrowing_variables_fixpoint();
    debug!(
        "Borrowing variables for method {} after fixpoint {:?}",
        method.name(),
        collector.borrowing_variables
    );
    // Filter out variables that are dereferenced.
    candidates.retain(|var| !collector.dereferenced_variables.contains(var));
    candidates.retain(|var| !collector.borrowing_variables.contains(var));
    debug!(
        "Variables in method {} to be purified {:?}",
        method.name(),
        candidates
    );

    let snapshots = encoder.get_snapshots();
    let mut purifier = Purifier::new(candidates, &*snapshots);

    for block in &mut method.basic_blocks {
        block.stmts = block
            .stmts
            .clone()
            .into_iter()
            .map(|stmt| StmtFolder::fold(&mut purifier, stmt))
            .collect();
    }

    for var in &mut method.local_vars {
        if purifier.vars.contains(&var.name) {
            let typ = std::mem::replace(&mut var.typ, vir::Type::Bool);
            var.typ = translate_type(typ, &*encoder.get_snapshots());
        } else if let Some(typ) = purifier.change_var_types.remove(&var.name) {
            var.typ = translate_type(typ, &*encoder.get_snapshots());
        }
    }

    method.local_vars.extend(purifier.fresh_variables);
}

pub fn translate_type(typ: vir::Type, snapshots: &HashMap<String, Box<Snapshot>>) -> vir::Type {
    match typ {
        vir::Type::TypedRef(name) => {
            let mut striped_name = name.as_str();
            while let Some(shorter) = striped_name.strip_prefix("ref$") {
                striped_name = shorter;
            }
            match striped_name {
                "i32" | "usize" | "u32" => vir::Type::Int,
                "bool" => vir::Type::Bool,
                _ => {
                    let domain_name = snapshots
                        .get(striped_name)
                        .and_then(|snap| snap.domain())
                        .map(|domain| domain.name)
                        .unwrap_or_else(|| {
                            panic!(
                                "No matching domain for '{}' in '{:?}'",
                                striped_name,
                                snapshots.keys(),
                            );
                        });
                    vir::Type::Domain(domain_name)
                }
            }
        }
        vir::Type::Domain(_) => {
            // Already translated.
            typ
        }
        x => unreachable!("we expect only references: {:?}", x),
    }
}

/// This is a ExprWalkerand StmtWalker used to collect information about which
/// local variables can be purified.
#[derive(Debug, Default)]
struct VarDependencyCollector {
    /// (Potentially) references that are dereferenced.
    dereferenced_variables: HashSet<String>,
    /// (Potentially) references that borrow other variables.
    borrowing_variables: HashSet<String>,
    /// Variables that are potentially reborrowed.
    dependencies: HashMap<String, HashSet<String>>,
    /// Variables that are potentially reborrowed.
    dependents: HashMap<String, HashSet<String>>,
}

impl VarDependencyCollector {
    /// Compute the fix-point of all dereferenced variables: dependencies of all
    /// dereferenced variables are also dereferenced variables.
    fn compute_dereferenced_variables_fixpoint(&mut self) {
        let mut changed = true;
        while changed {
            let mut add_queue = Vec::new();
            for var in &self.dereferenced_variables {
                if let Some(dependencies) = self.dependencies.remove(var) {
                    add_queue.push(dependencies);
                }
            }
            changed = !add_queue.is_empty();
            for dependencies in add_queue {
                self.dereferenced_variables.extend(dependencies);
            }
        }
    }
    /// Compute the fix-point of all borrowing variables: dependents of all
    /// borrowing variables are also borrowing variables.
    fn compute_borrowing_variables_fixpoint(&mut self) {
        let mut changed = true;
        while changed {
            let mut add_queue = Vec::new();
            for var in &self.borrowing_variables {
                if let Some(dependents) = self.dependents.remove(var) {
                    add_queue.push(dependents);
                }
            }
            changed = !add_queue.is_empty();
            for dependents in add_queue {
                self.borrowing_variables.extend(dependents);
            }
        }
    }
}

impl ExprWalker for VarDependencyCollector {
    fn walk_field(&mut self, receiver: &vir::Expr, _field: &vir::Field, _pos: &vir::Position) {
        match receiver {
            // If we have a variable that is accessed two levels down, we assume
            // that it is dereferenced without checking the actual type.
            vir::Expr::Field(box vir::Expr::Local(local_var, _), _, _) => {
                self.dereferenced_variables.insert(local_var.name.clone());
            }
            _ => ExprWalker::walk(self, receiver),
        }
    }
}

impl StmtWalker for VarDependencyCollector {
    fn walk_expr(&mut self, expr: &vir::Expr) {
        ExprWalker::walk(self, expr);
    }
    fn walk_assign(&mut self, target: &vir::Expr, source: &vir::Expr, kind: &vir::AssignKind) {
        let dependencies = collect_variables(source);
        let dependents = collect_variables(target);
        for dependent in &dependents {
            let entry = self.dependencies.entry(dependent.clone()).or_insert(HashSet::new());
            entry.extend(dependencies.iter().cloned());
        }
        for dependency in dependencies {
            let entry = self.dependents.entry(dependency).or_insert(HashSet::new());
            entry.extend(dependents.iter().cloned());
        }
        match kind {
            vir::AssignKind::SharedBorrow(_) |
            vir::AssignKind::MutableBorrow(_) => {
                match target {
                    vir::Expr::Field(box vir::Expr::Local(local_var, _), _, _) => {
                        match source {
                            vir::Expr::Field(box vir::Expr::Local(_, _), vir::Field { name, .. }, _)
                                if name == "val_ref" => {
                                // Reborrowing is fine.
                            }
                            _ => {
                                self.borrowing_variables.insert(local_var.name.clone());
                            }
                        }
                    }
                    _ => {},
                }
            }
            _ => {}
        }
        self.walk_expr(target);
        self.walk_expr(source);
    }
}

fn collect_variables(expr: &vir::Expr) -> HashSet<String> {
    let mut collector = VariableCollector { vars: HashSet::new() };
    ExprWalker::walk(&mut collector, expr);
    collector.vars
}

struct VariableCollector {
    vars: HashSet<String>,
}

impl ExprWalker for VariableCollector {
    fn walk_local(&mut self, local_var: &vir::LocalVar, _pos: &vir::Position) {
        if !self.vars.contains(&local_var.name) {
            self.vars.insert(local_var.name.clone());
        }
    }
}

struct Purifier<'a> {
    vars: HashSet<String>,
    inline_snap_functions: HashMap<String, vir::LocalVar>,
    fresh_variables: Vec<vir::LocalVar>,
    snapshots: &'a HashMap<String, Box<Snapshot>>,
    change_var_types: HashMap<String, vir::Type>,
}

impl<'a> Purifier<'a> {
    fn new(vars: HashSet<String>, snapshots: &'a HashMap<String, Box<Snapshot>>) -> Self {
        Self { vars, inline_snap_functions: HashMap::new(), fresh_variables: Vec::new(), snapshots,
            change_var_types: HashMap::new() }
    }
    fn fresh_variable(&mut self, typ: &vir::Type) -> vir::LocalVar {
        let name = format!("havoc${}", self.fresh_variables.len());
        let var = vir::LocalVar {
            name,
            typ: translate_type(typ.clone(), self.snapshots),
        };
        self.fresh_variables.push(var.clone());
        var
    }
}

impl<'a> StmtFolder for Purifier<'a> {
    fn fold_expr(&mut self, expr: vir::Expr) -> vir::Expr {
        ExprFolder::fold(self, expr)
    }
    fn fold_method_call(
        &mut self,
        name: String,
        args: Vec<vir::Expr>,
        targets: Vec<vir::LocalVar>
    ) -> vir::Stmt {
        match targets.as_slice() {
            [local_var] if self.vars.contains(&local_var.name) => {
                return vir::Stmt::Assign(
                    vir::LocalVar {
                        name: local_var.name.clone(),
                        typ: translate_type(local_var.typ.clone(), self.snapshots)
                    }.into(),
                    self.fresh_variable(&local_var.typ).into(),
                    vir::AssignKind::Ghost
                );
            }
            _ => {}
        }
        vir::Stmt::MethodCall(
            name,
            args.into_iter().map(|e| self.fold_expr(e)).collect(),
            targets
        )
    }
    fn fold_assign(&mut self, target: vir::Expr, source: vir::Expr, kind: vir::AssignKind) -> vir::Stmt {
        let mut target = self.fold_expr(target);
        let mut source = self.fold_expr(source);
        match (&mut target, &mut source) {
            (vir::Expr::Local(target_var, _), vir::Expr::Local(source_var, _))
                    if (target_var.name.starts_with("_preserve") ||
                        target_var.name.starts_with("_old$")
                        ) && self.vars.contains(&source_var.name) => {
                target_var.typ = translate_type(source_var.typ.clone(), self.snapshots);
                self.change_var_types.insert(target_var.name.clone(), source_var.typ.clone());
            }
            _ => {}
        }
        vir::Stmt::Assign(target, source, kind)
    }
}

impl<'a> ExprFolder for Purifier<'a> {
    fn fold_field_access_predicate(
        &mut self,
        receiver: Box<vir::Expr>,
        perm_amount: vir::PermAmount,
        pos: vir::Position
    ) -> vir::Expr {
        match &*receiver {
            vir::Expr::Field(box vir::Expr::Local(local_var, _), _, _)
                    if self.vars.contains(&local_var.name) => {
                return true.into();
            }
            _ => {}
        }
        vir::Expr::FieldAccessPredicate(receiver, perm_amount, pos)
    }
    fn fold_predicate_access_predicate(
        &mut self,
        name: String,
        arg: Box<vir::Expr>,
        perm_amount: vir::PermAmount,
        pos: vir::Position
    ) -> vir::Expr {
        let arg = self.fold_boxed(arg);
        match &*arg {
            vir::Expr::Local(local_var, _)
                    if self.vars.contains(&local_var.name) ||
                        self.change_var_types.contains_key(&local_var.name) => {
                return true.into();
            }
            _ => {}
        }
        vir::Expr::PredicateAccessPredicate(name, arg, perm_amount, pos)
    }
    fn fold_labelled_old(
        &mut self,
        label: String,
        body: Box<vir::Expr>,
        pos: vir::Position
    ) -> vir::Expr {
        let body = self.fold_boxed(body);
        if !body.is_heap_dependent() {
            return *body;
        }
        vir::Expr::LabelledOld(label, body, pos)
    }
    fn fold_local(&mut self, mut var: vir::LocalVar, pos: vir::Position) -> vir::Expr {
        if let Some(new_type) = self.change_var_types.get(&var.name) {
            var.typ = translate_type(new_type.clone(), self.snapshots);
        }
        vir::Expr::Local(var, pos)
    }
    fn fold_field(&mut self, receiver: Box<vir::Expr>, field: vir::Field, pos: vir::Position) -> vir::Expr {
        match receiver {
            box vir::Expr::Local(local_var, local_pos) if self.vars.contains(&local_var.name) => {
                return vir::LocalVar {
                    name: local_var.name,
                    typ: translate_type(local_var.typ, self.snapshots),
                }.into();
            }
            _ => {}
        }
        vir::Expr::Field(receiver, field, pos)
    }
    fn fold_func_app(
        &mut self,
        name: String,
        args: Vec<vir::Expr>,
        formal_args: Vec<vir::LocalVar>,
        return_type: vir::Type,
        pos: vir::Position
    ) -> vir::Expr {
        let args: Vec<_> = args.into_iter().map(|e| ExprFolder::fold(self, e)).collect();
        if name.starts_with("snap$") {
            match args.as_slice() {
                [vir::Expr::Local(local_var, local_pos)] => {
                    if self.vars.contains(&local_var.name) ||
                            self.change_var_types.contains_key(&local_var.name) {
                        return vir::Expr::Local(local_var.clone(), local_pos.clone());
                    }
                }
                _ => {}
            }
        }
        vir::Expr::FuncApp(
            name,
            args,
            formal_args,
            return_type,
            pos
        )
    }
}