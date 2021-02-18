// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::config;
use std::io::Write;
use crate::vir::{self, cfg::method::*};

fn escape_html<S: ToString>(s: S) -> String {
    s.to_string()
        .replace("&", "&amp;")
        .replace(">", "&gt;")
        .replace("<", "&lt;")
        .replace("\n", "<br/>")
}

impl CfgMethod {
    pub fn to_graphviz(&self, graph: &mut dyn Write) {
        self.to_graphviz_with_extra(graph, |_| vec![])
    }

    pub fn to_graphviz_with_extra<F: Fn(usize) -> Vec<String>>(&self, graph: &mut dyn Write, extra: F) {
        writeln!(graph, "digraph CFG {{").unwrap();
        writeln!(graph, "graph [fontname=monospace];").unwrap();
        writeln!(graph, "node [fontname=monospace];").unwrap();
        writeln!(graph, "edge [fontname=monospace];").unwrap();

        // Add title
        writeln!(graph, "labelloc=\"t\";").unwrap();
        writeln!(graph, "label=\"Method {}\";", escape_html(self.name())).unwrap();

        for (index, block) in self.basic_blocks.iter().enumerate() {
            let label = self.index_to_label(index);
            let extra_rows = extra(index);
            let (content, reborrowing_dags) =
                self.block_to_graphviz(index, &label, block, &extra_rows, &[]);
            writeln!(
                graph,
                "\"block_{}\" [shape=none,label=<{}>];",
                escape_html(&label),
                content
            )
            .unwrap();

            if config::dump_reborrowing_dag_in_debug_info() {
                for dag in reborrowing_dags {
                    writeln!(graph, "subgraph cluster_{} {{", label).unwrap();
                    writeln!(graph, "   label=\"Reborrowing DAG {}\"", label).unwrap();
                    for node in dag.iter() {
                        writeln!(
                            graph,
                            "dag_{}_node_{:?} [shape=none,label=<",
                            label, node.borrow
                        )
                        .unwrap();
                        writeln!(graph, "<table>").unwrap();
                        writeln!(
                            graph,
                            "<tr><td colspan=\"2\">{:?} (guard: {})</td></tr>",
                            node,
                            escape_html(&dag.guard(node.borrow))
                        )
                        .unwrap();
                        for (i, stmt) in node.stmts.iter().enumerate() {
                            writeln!(
                                graph,
                                "<tr><td>{}</td><td>{}</td></tr>",
                                i,
                                escape_html(stmt)
                            )
                            .unwrap();
                        }
                        writeln!(graph, "</table>").unwrap();
                        writeln!(graph, ">];").unwrap();
                        for r_node in &node.reborrowing_nodes {
                            writeln!(
                                graph,
                                "\"dag_{}_node_{:?}\" -> \"dag_{}_node_{:?}\";",
                                label, r_node, label, node.borrow
                            )
                            .unwrap();
                        }
                        for r_node in &node.reborrowed_nodes {
                            writeln!(
                                graph,
                                "\"dag_{}_node_{:?}\" -> \"dag_{}_node_{:?}\";",
                                label, node.borrow, label, r_node
                            )
                            .unwrap();
                        }
                    }
                    writeln!(graph, "}}").unwrap();

                    let node = dag.iter().next().unwrap();
                    writeln!(
                        graph,
                        "\"block_{}\" -> \"dag_{}_node_{:?}\" [dir=none lhead=cluster_{}];",
                        label, label, node.borrow, label,
                    )
                    .unwrap();
                }
            }
        }

        for (index, block) in self.basic_blocks.iter().enumerate() {
            let block_label = self.index_to_label(index);
            for target in block.successor.get_following() {
                let target_label = self.index_to_label(target.block_index);
                writeln!(
                    graph,
                    "\"block_{}\" -> \"block_{}\";",
                    escape_html(&block_label),
                    escape_html(&target_label),
                )
                .unwrap();
            }
        }

        writeln!(graph, "}}").unwrap();
    }

    fn index_to_label(&self, index: usize) -> String {
        self.basic_blocks_labels[index].clone()
    }

    fn block_to_graphviz<'b>(
        &self,
        index: usize,
        label: &str,
        block: &'b CfgBlock,
        header_rows: &[String],
        footer_rows: &[String],
    ) -> (String, Vec<&'b vir::borrows::DAG>) {
        let mut reborrowing_dags = Vec::new();
        let mut lines: Vec<String> = vec![];
        lines.push("<table border=\"0\" cellborder=\"1\" cellspacing=\"0\">".to_string());

        lines.push("<tr><td bgcolor=\"gray\" align=\"center\">".to_string());
        lines.push(format!("{} (cfg:{})", escape_html(&label), index));
        lines.push("</td></tr>".to_string());

        for row in header_rows {
            lines.push(
                "<tr><td align=\"left\" balign=\"left\"><font color=\"steelblue\">".to_string(),
            );
            lines.push(escape_html(row));
            lines.push("</font></td></tr>".to_string());
        }

        lines.push("<tr><td align=\"left\" balign=\"left\">".to_string());
        let mut first_row = true;
        for stmt in &block.stmts {
            if !first_row {
                lines.push("<br/>".to_string());
            }
            first_row = false;
            match stmt {
                vir::Stmt::ExpireBorrows(ref dag) => {
                    reborrowing_dags.push(dag);
                }
                vir::Stmt::PackageMagicWand(_, ref stmts, _, _, _) => {
                    for stmt in stmts {
                        if let vir::Stmt::ExpireBorrows(ref dag) = stmt {
                            reborrowing_dags.push(dag);
                        }
                    }
                }
                _ => {}
            }
            let stmt_string = stmt.to_string();
            let mut splitted_stmt_lines = vec![];
            for stmt_line in stmt_string.lines() {
                splitted_stmt_lines.push(
                    sub_strings(stmt_line, 120, 116)
                        .into_iter()
                        .map(|x| escape_html(x))
                        .collect::<Vec<_>>()
                        .join(" \\ <br/>    "),
                );
            }
            let stmt_html = splitted_stmt_lines.join("<br/>");
            if stmt.is_comment() {
                lines.push(format!("<font color=\"orange\">{}</font>", stmt_html));
            } else {
                lines.push(stmt_html);
            }
        }
        lines.push("</td></tr>".to_string());

        match block.successor {
            // Red
            Successor::Undefined => {
                lines.push("<tr><td align=\"left\" bgcolor=\"#F43E3E\">".to_string())
            }
            // Green
            Successor::Return => {
                lines.push("<tr><td align=\"left\" bgcolor=\"#82CA9D\">".to_string())
            }
            _ => lines.push("<tr><td align=\"left\">".to_string()),
        }
        {
            let full_successor = block.successor.to_string();
            let splitted_successor = sub_strings(&full_successor, 120, 116);
            lines.push(
                splitted_successor
                    .into_iter()
                    .map(|x| escape_html(x))
                    .collect::<Vec<_>>()
                    .join(" \\ <br/>    "),
            );
        }
        lines.push("</td></tr>".to_string());

        for row in footer_rows {
            lines.push(
                "<tr><td align=\"left\" balign=\"left\"><font color=\"steelblue\">".to_string(),
            );
            lines.push(escape_html(row));
            lines.push("</font></td></tr>".to_string());
        }

        lines.push("</table>".to_string());

        (lines.join(""), reborrowing_dags)
    }
}

fn sub_strings(string: &str, first_sub_len: usize, sub_len: usize) -> Vec<&str> {
    let mut subs = Vec::with_capacity(string.len() / sub_len);
    let mut iter = string.chars();
    let mut pos = 0;
    let mut is_first = true;

    while pos < string.len() {
        let mut len = 0;
        if is_first {
            for ch in iter.by_ref().take(first_sub_len) {
                len += ch.len_utf8();
            }
            is_first = false;
        } else {
            for ch in iter.by_ref().take(sub_len) {
                len += ch.len_utf8();
            }
        }
        subs.push(&string[pos..pos + len]);
        pos += len;
    }
    subs
}
