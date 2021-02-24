// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![feature(rustc_private)]
#![feature(box_patterns)]
#![feature(box_syntax)]

#![allow(unused_imports)]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
// #![deny(unused_mut)]
// #![deny(unused_variables)]
// #![deny(unused_imports)]
// #![deny(unused_doc_comments)]

extern crate rustc_middle;
extern crate rustc_hir;
extern crate rustc_span;
extern crate rustc_index;
extern crate rustc_ast;
extern crate rustc_target;
extern crate rustc_attr;
#[macro_use]
extern crate lazy_static;

pub mod encoder;
mod utils;
pub mod verifier;
