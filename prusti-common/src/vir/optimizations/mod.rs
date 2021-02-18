// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! A module that contains various VIR optimizations.

use crate::config::optimizations;
use crate::vir::{CfgMethod, Program};
use crate::config::{self, Optimizations};

pub mod folding;
pub mod functions;
pub mod methods;
pub mod predicates;

fn log_method(
    source_file_name: &str,
    cfg: &CfgMethod,
    optimization_name: &str,
    after_optimization: bool
) {
    if config::dump_debug_info() {
        let namespace = format!(
            "graphviz_method_optimization_{}_{}",
            optimization_name,
            if after_optimization { "after" } else { "before" }
        );
        crate::report::log::report_with_writer(
            &namespace,
            format!("{}.{}.dot", source_file_name, cfg.name()),
            |writer| cfg.to_graphviz(writer),
        );
    }
}

fn log_methods(
    source_file_name: &str,
    cfgs: &[CfgMethod],
    optimization_name: &str,
    after_optimization: bool
) {
    for cfg in cfgs {
        log_method(source_file_name, cfg, optimization_name, after_optimization);
    }
}

impl Program {
    pub fn optimized(mut self, source_file_name: &str) -> Self {
        let optimizations = config::optimizations();
        info!("Enabled optimisations: {:?}", optimizations);

        // can't borrow self because we need to move fields
        if optimizations.inline_constant_functions {
            log_methods(
                source_file_name,
                &self.methods,
                "inline_constant_functions",
                false
            );
            let (new_methods, new_functions) =
                functions::inline_constant_functions(self.methods, self.functions);
            self.methods = new_methods;
            self.functions = new_functions;
            log_methods(
                source_file_name,
                &self.methods,
                "inline_constant_functions",
                true
            );
        }
        if optimizations.optimize_folding {
            log_methods(
                source_file_name,
                &self.methods,
                "folding",
                false
            );
            self.methods = self.methods
                .into_iter()
                .map(|cfg| {
                    folding::FoldingOptimizer::optimize(cfg)
                })
                .collect();
            self.functions = self.functions
                .into_iter()
                .map(|f| folding::FoldingOptimizer::optimize(f))
                .collect();
            log_methods(
                source_file_name,
                &self.methods,
                "folding",
                true
            );
        }
        self.methods = self.methods.into_iter().map(|method| {
            methods::optimize_method_encoding(method, source_file_name, &optimizations)
        }).collect();
        if optimizations.delete_unused_predicates {
            self.viper_predicates = predicates::delete_unused_predicates(
                &self.methods,
                &self.functions,
                self.viper_predicates,
            );
        }

        self
    }
}
