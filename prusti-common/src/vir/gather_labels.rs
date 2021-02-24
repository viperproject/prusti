use crate::vir::{StmtWalker, ast};

pub(super) fn gather_labels(stmt: &ast::Stmt) -> Vec<String> {
    let mut gather_labels = GatherLabels::default();
    gather_labels.walk(stmt);
    gather_labels.labels
}

#[derive(Default)]
struct GatherLabels {
    labels: Vec<String>
}

impl StmtWalker for GatherLabels {
    fn walk_label(&mut self, label: &str) {
        self.labels.push(label.to_string());
    }
}
