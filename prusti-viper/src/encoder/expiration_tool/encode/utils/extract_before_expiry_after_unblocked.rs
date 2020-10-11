use std::collections::HashSet;

use prusti_common::vir;
use prusti_common::vir::ExprFolder;

use crate::utils::namespace::Namespace;

use super::super::Binding;
use super::super::super::Context;

/// Finds all sub-expressions of the form `before_expiry(e)` and `after_unblocked(e)` and replaces
/// them with a fresh variable, sourced from the `namespace` argument. It returns a list of
/// performed replacements in the form of bindings.
///
/// Concretely, every `before_expiry(ex)` that is replaced by a variable `fx` produces a binding
/// `(fx, BeforeExpiry, ex)`, and every `after_unblocked(ey)` that is replaced by a variable `fy`
/// produces a binding `(fy, AfterUnblocked, ey)`.
pub(in super::super) fn extract_before_expiry_after_unblocked(
    pledge: vir::Expr,
    namespace: &mut Namespace
) -> (vir::Expr, HashSet<Binding>) {
    let mut extractor = Extractor { namespace, replacements: HashSet::new() };
    let pledge = extractor.fold(pledge);
    let replacements = extractor.replacements;
    (pledge, replacements)
}

struct Extractor<'a> {
    namespace: &'a mut Namespace,
    replacements: HashSet<Binding>
}

impl<'a> Extractor<'a> {
    fn next_var(&mut self) -> vir::LocalVar {
        vir::LocalVar::new(self.namespace.next(), vir::Type::Int)
    }
}

impl<'a> ExprFolder for Extractor<'a> {
    fn fold_labelled_old(&mut self,
        label: String,
        body: Box<vir::Expr>,
        pos: vir::Position
    ) -> vir::Expr {
        let context = match label.as_ref() {
            "before_expiry/xxx" => Context::BeforeExpiry,
            "after_unblocked/xxx" => Context::AfterUnblocked,
            _ => return vir::Expr::LabelledOld(label, body, pos),
        };

        let var = self.next_var();
        self.replacements.insert(Binding(var.clone(), context, *body));

        vir::Expr::Local(var, pos)
    }
}

