// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::fmt;
use crate::polymorphic::cfg::method::*;

impl fmt::Display for CfgMethod {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(
            f,
            "method {}({} args)",
            self.method_name, self.formal_arg_count
        )?;
        writeln!(
            f,
            "    returns ({})",
            self.formal_returns
                .iter()
                .map(|x| format!("{:?}", x))
                .collect::<Vec<String>>()
                .join(", ")
        )?;
        writeln!(f, "{{")?;
        for local_var in self.local_vars.iter() {
            writeln!(f, "    {:?}", local_var)?;
        }

        for (index, block) in self.basic_blocks.iter().enumerate() {
            writeln!(
                f,
                "  label {} // {}",
                self.basic_blocks_labels[index], index
            )?;
            for stmt in &block.stmts {
                writeln!(f, "    {}", stmt)?;
            }
            writeln!(f, "    {:?}", block.successor)?;
        }
        writeln!(f, "  label {}", RETURN_LABEL)?;
        writeln!(f, "}}")
    }
}

impl fmt::Display for Successor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Successor::Undefined => writeln!(f, "Undefined"),
            Successor::Return => writeln!(f, "Return"),
            Successor::Goto(ref target) => writeln!(f, "Goto({})", target),
            Successor::GotoSwitch(ref guarded_targets, ref default_target) => writeln!(
                f,
                "GotoSwitch({}, {})",
                guarded_targets
                    .iter()
                    .map(|(guard, target)| format!("({}, {})", guard, target))
                    .collect::<Vec<_>>()
                    .join(", "),
                default_target
            ),
        }
    }
}

impl fmt::Display for CfgBlockIndex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "cfg:{}", self.block_index)
    }
}
