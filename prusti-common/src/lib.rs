// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![feature(box_patterns)]
#![feature(box_syntax)]
#![feature(nll)]

#![allow(warnings)]

#[macro_use]
extern crate log;
extern crate config as config_crate;
extern crate itertools;
#[macro_use]
extern crate lazy_static;
extern crate regex;
#[macro_use]
extern crate serde;
extern crate uuid;
extern crate viper;

extern crate prusti_utils;

pub mod config;
pub mod report;
mod stopwatch;
pub mod utils;
pub mod verification_context;
pub mod verification_service;
pub mod vir;

pub use stopwatch::Stopwatch;
