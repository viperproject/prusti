// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! This module defines functions for user output: the nice, clean, readable, polished output

use crate::config;

/// Print to stderr a message that is only meant to be read by the user.
/// The output goes to *stderr*, to be displayed in the "compiler output" area in rust-playground.
/// Note: this function does nothing if `config::quiet()` is true.
pub fn message<S: ToString>(msg: S) {
    if !config::quiet() {
        eprintln!("{}", msg.to_string())
    }
}
