// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Optimization that removes unused if statements.

use std::slice;

use crate::vir::{cfg, Stmt};

/// Remove empty if statements:
/// * `if (...) {}`
/// * `if (...) { /* ... */ }`
/// * `if (...) { if (...) {}; if (...) { /* ... */ } }`
pub fn remove_empty_if(mut method: cfg::CfgMethod) -> cfg::CfgMethod {
    method.retain_stmts(|stmt| {
        match stmt {
            Stmt::If(_, _, _) => !is_empty_body(slice::from_ref(stmt)),
            _ => true, // Keep the rest
        }
    });
    method
}

fn is_empty_body(stmts: &[Stmt]) -> bool {
    stmts.iter().all(|stmt| match stmt {
        Stmt::Comment(_) |
        Stmt::TransferPerm(..) => true,
        Stmt::If(_, ref then_stmts, ref else_stmts) =>
            is_empty_body(then_stmts) && is_empty_body(else_stmts),
        _ => false
    })
}
