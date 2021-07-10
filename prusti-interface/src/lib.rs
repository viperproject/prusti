// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Prusti Interface is an interface between Prusti and Prusti-Viper.

#![allow(unused_imports)]
#![deny(unused_must_use)]
#![deny(unsafe_op_in_unsafe_fn)]

// #![deny(unused_imports)]
// #![deny(unused_mut)]
// #![deny(non_upper_case_globals)]
// #![allow(missing_docs)]
#![feature(rustc_private)]
#![feature(box_syntax)]
#![feature(box_patterns)]

// #![feature(nll)]

// #![feature(try_from)]
// #![feature(crate_in_paths)]
// #![feature(map_get_key_value)]

extern crate config as config_crate;
extern crate rustc_hir;
extern crate rustc_middle;
extern crate rustc_span;
extern crate rustc_ast;
extern crate rustc_attr;

// extern crate csv;
// extern crate datafrog;
// #[macro_use]
// extern crate log;
// extern crate polonius;
extern crate polonius_engine;
// extern crate regex;
// extern crate rustc;
extern crate rustc_data_structures;
extern crate rustc_index;
extern crate rustc_trait_selection;
// extern crate rustc_driver;
// extern crate rustc_hash;
extern crate rustc_mir;
// extern crate rustc_target;
// #[macro_use]
// extern crate serde;
// extern crate serde;
// extern crate syntax;
// extern crate syntax_pos;
#[macro_use]
extern crate lazy_static;

pub mod data;
pub mod environment;
pub mod specs;
pub mod utils;

pub use prusti_error::*;

mod prusti_error;
