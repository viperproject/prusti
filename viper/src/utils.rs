// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Various utility methods for working with Viper.

use crate::ast_factory::{AstFactory, Expr};

pub trait ExprIterator<'v> {
    /// Conjoin a sequence of expressions into a single expression.
    /// Panics if the sequence has no elements.
    fn conjoin(&mut self, ast: &AstFactory<'v>) -> Expr<'v>;

    /// Conjoin a sequence of expressions into a single expression.
    fn conjoin_with_init(&mut self, ast: &AstFactory<'v>, init: Expr<'v>) -> Expr<'v>;
}

impl<'v, T> ExprIterator<'v> for T
where
    T: Iterator<Item = Expr<'v>>,
{
    fn conjoin(&mut self, ast: &AstFactory<'v>) -> Expr<'v> {
        let init = self.next().unwrap();
        self.conjoin_with_init(ast, init)
    }

    fn conjoin_with_init(&mut self, ast: &AstFactory<'v>, init: Expr<'v>) -> Expr<'v> {
        self.fold(init, |acc, conjunct| ast.and(acc, conjunct))
    }
}

// copied from prusti-common to avoid illogical dependency.
/// Runs statements on the same level as the macro call, timing and logging (info-level by default) how long it took.
#[macro_export]
macro_rules! run_timed {
    ($desc:expr, $($s:stmt;)*) => {
        run_timed!($desc, info, $($s;)*);
    };
    ($desc:expr, $log_level:ident, $($s:stmt;)*) => {
        $log_level!("Starting: {}", $desc);
        let start = ::std::time::Instant::now();
        $($s)*
        let duration = start.elapsed();
        $log_level!(
            "Completed: {} ({}.{} seconds)",
            $desc,
            duration.as_secs(),
            duration.subsec_millis() / 10
        );
    };
}
