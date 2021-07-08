use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{self, BufWriter, Write},
    path::Path,
};
use super::{MirBody, compiler::{Statement, Terminator}};
use rustc_middle::{mir, ty, ty::TyCtxt};

struct GraphvizFile {
    output: std::cell::RefCell<BufWriter<File>>,
}

impl GraphvizFile {
    fn new(path: &Path) -> IoResult<Self> {
        let output_file = File::create(path)?;
        let output = std::cell::RefCell::new(BufWriter::new(output_file));
        Ok(Self {
            output
        })
    }
}

struct GraphvizWriter<'tcx, 'a> {
    output: GraphvizFile,
    body: &'a MirBody<'tcx>,
}

macro_rules! write_graph {
    ( $self:ident, $( $x:expr ),* ) => {
        writeln!($self.output.output.borrow_mut(), $( $x ),*)?;
    }
}

macro_rules! write_edge {
    ( $self:ident, $source:ident, str $target:ident ) => {{
        write_graph!($self, "\"{:?}\" -> \"{}\"\n", $source, stringify!($target));
    }};
    ( $self:ident, $source:ident, unwind $target:ident ) => {{
        write_graph!(
            $self,
            "\"{:?}\" -> \"{:?}\" [color=red]\n",
            $source,
            $target
        );
    }};
    ( $self:ident, $source:ident, imaginary $target:ident ) => {{
        write_graph!(
            $self,
            "\"{:?}\" -> \"{:?}\" [style=\"dashed\"]\n",
            $source,
            $target
        );
    }};
    ( $self:ident, $source:ident, $target:ident ) => {{
        write_graph!($self, "\"{:?}\" -> \"{:?}\"\n", $source, $target);
    }};
}

macro_rules! to_html {
    ( $o:expr ) => {{
        format!("{:?}", $o)
            .replace("{", "\\{")
            .replace("}", "\\}")
            .replace("&", "&amp;")
            .replace(">", "&gt;")
            .replace("<", "&lt;")
            .replace("\n", "<br/>")
    }};
}

macro_rules! to_html_display {
    ( $o:expr ) => {{
        format!("{}", $o)
            .replace("{", "\\{")
            .replace("}", "\\}")
            .replace("&", "&amp;")
            .replace(">", "&gt;")
            .replace("<", "&lt;")
            .replace("\n", "<br/>")
    }};
}

macro_rules! to_sorted_string {
    ( $o:expr ) => {{
        let mut vector = $o.iter().map(|x| to_html!(x)).collect::<Vec<String>>();
        vector.sort();
        vector.join(", ")
    }};
}

impl<'tcx, 'a> GraphvizWriter<'tcx, 'a> {
    fn new(path: &Path, body: &'a MirBody<'tcx>) -> IoResult<Self> {
        let output = GraphvizFile::new(path)?;
        Ok(Self {
            output, body
        })
    }
    fn write_all(&self) -> IoResult {
        write_graph!(self, "digraph G {{\n");
        self.universal_lifetimes()?;
        self.print_temp_variables()?;
        for bb in self.body.basic_block_indices() {
            self.visit_basic_block(bb)?;
        }
        write_graph!(self, "}}\n");
        Ok(())
    }
    fn universal_lifetimes(&self) -> IoResult {
        write_graph!(self, "universal_lifetimes [ style=filled shape = \"record\"");
        write_graph!(self, "label =<<table>");
        write_graph!(self, "<tr><td>Universal lifetimes</td></tr>");
        write_graph!(
            self,
            "<tr><td>{}</td></tr>",
            to_sorted_string!(self.body.get_universal_lifetimes())
        );
        write_graph!(self, "<tr><td>Constraints</td></tr>");
        for constraint in self.body.get_universal_lifetime_constraints() {
            write_graph!(
                self,
                "<tr><td>{}</td></tr>",
                to_html_display!(constraint)
            );
        }
        write_graph!(self, "</table>>];");
        Ok(())
    }
    fn print_temp_variables(&self) -> IoResult {
        write_graph!(self, "Variables [ style=filled shape = \"record\"");
        write_graph!(self, "label =<<table>");
        write_graph!(self, "<tr><td colspan=\"3\">VARIABLES</td></tr>");
        write_graph!(
            self,
            "<tr><td>Name</td><td>Temporary</td><td>Type</td><td>Region</td></tr>"
        );
        for local in self.body.iter_locals() {
            write_graph!(
                self,
                "<tr><td>{}</td><td>{:?}</td><td>{}</td><td>{}</td></tr>",
                local.name().unwrap_or(""),
                local.id(),
                to_html!(local.ty()),
                "todo: region"
            );
        }
        write_graph!(self, "</table>>];");
        Ok(())
    }
    fn visit_basic_block(&self, bb: mir::BasicBlock) -> Result<(), io::Error> {
        write_graph!(self, "\"{:?}\" [ shape = \"record\"", bb);
        write_graph!(self, "label =<<table>");
        write_graph!(self, "<th>");
        write_graph!(self, "<td>{:?}</td>", bb);
        write_graph!(self, "<td></td>");
        write_graph!(self, "</th>");

        write_graph!(self, "<th>");
        write_graph!(self, "<td>Nr</td>");
        write_graph!(self, "<td>statement</td>");
        write_graph!(self, "<td>starting lifetimes</td>");
        write_graph!(self, "<td>new lifetime constraints</td>");
        write_graph!(self, "<td>ending lifetimes</td>");
        write_graph!(self, "<td>outlives start</td>");
        write_graph!(self, "<td>outlives mid</td>");
        write_graph!(self, "</th>");

        let block = self.body.get_block(bb);
        for statement in block.iter_statements() {
            self.visit_statement(&statement)?;
        }
        let terminator = block.terminator();
        let term_str = if let Some(term) = &terminator {
            to_html!(term.kind())
        } else {
            String::from("")
        };
        write_graph!(self, "<tr>");
        write_graph!(self, "<td></td>");
        write_graph!(self, "<td>{}</td>", term_str);
        write_graph!(self, "</tr>");
        write_graph!(self, "</table>> ];");

        if let Some(terminator) = &terminator {
            self.visit_terminator(terminator)?;
        }

        Ok(())
    }

    fn visit_statement(
        &self,
        statement: &Statement,
    ) -> Result<(), io::Error> {
        write_graph!(self, "<tr>");
        write_graph!(self, "<td>{}</td>", statement.index());
        write_graph!(self, "<td>{}</td>", to_html!(statement.kind()));
        write_graph!(self, "<td>TODO</td>");
        write_graph!(self, "<td>TODO</td>");
        write_graph!(self, "<td>TODO</td>");
        write_graph!(self, "<td>{}</td>", to_sorted_string!(self.body.get_outlives_at_start(statement.location())));
        write_graph!(self, "<td>{}</td>", to_sorted_string!(self.body.get_outlives_at_mid(statement.location())));
        write_graph!(self, "</tr>");
        Ok(())
    }

    fn visit_terminator(
        &self,
        terminator: &Terminator,
    ) -> Result<(), io::Error> {
        use rustc_middle::mir::TerminatorKind;
        let bb = terminator.basic_block();
        match terminator.kind() {
            TerminatorKind::Goto { target } => {
                write_edge!(self, bb, target);
            }
            TerminatorKind::SwitchInt { ref targets, .. } => {
                for target in targets.all_targets() {
                    write_edge!(self, bb, target);
                }
            }
            TerminatorKind::Resume => {
                write_edge!(self, bb, str resume);
            }
            TerminatorKind::Abort => {
                write_edge!(self, bb, str abort);
            }
            TerminatorKind::Return => {
                write_edge!(self, bb, str return);
            }
            TerminatorKind::Unreachable => {}
            TerminatorKind::DropAndReplace {
                ref target, unwind, ..
            }
            | TerminatorKind::Drop {
                ref target, unwind, ..
            } => {
                write_edge!(self, bb, target);
                if let Some(target) = unwind {
                    write_edge!(self, bb, unwind target);
                }
            }
            TerminatorKind::Call {
                ref destination,
                cleanup,
                ..
            } => {
                if let &Some((_, target)) = destination {
                    write_edge!(self, bb, target);
                }
                if let Some(target) = cleanup {
                    write_edge!(self, bb, unwind target);
                }
            }
            TerminatorKind::Assert {
                target, cleanup, ..
            } => {
                write_edge!(self, bb, target);
                if let Some(target) = cleanup {
                    write_edge!(self, bb, unwind target);
                }
            }
            TerminatorKind::Yield { .. } => unimplemented!(),
            TerminatorKind::GeneratorDrop => unimplemented!(),
            TerminatorKind::FalseEdge {
                ref real_target,
                ref imaginary_target,
            } => {
                write_edge!(self, bb, real_target);
                write_edge!(self, bb, imaginary_target);
            }
            TerminatorKind::FalseUnwind {
                real_target,
                unwind,
            } => {
                write_edge!(self, bb, real_target);
                if let Some(target) = unwind {
                    write_edge!(self, bb, imaginary target);
                }
            }
            TerminatorKind::InlineAsm { .. } => unimplemented!(),
        };
        Ok(())
    }
}

type IoResult<T=()> = Result<T, io::Error>;

impl<'tcx> MirBody<'tcx> {
    pub fn to_graphviz(&self, path: &Path) -> IoResult
    {
        let writer = GraphvizWriter::new(path, self)?;
        writer.write_all()?;
        Ok(())
    }
}