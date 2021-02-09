// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![deny(unused_must_use)]

#[macro_use]
extern crate error_chain;
extern crate jni;
#[macro_use]
extern crate log;
extern crate uuid;
extern crate viper_sys;
#[macro_use]
extern crate serde;

mod ast_factory;
mod ast_utils;
pub mod errors;
mod jni_utils;
#[macro_use]
pub mod utils;
mod verification_backend;
mod verification_context;
mod verification_result;
mod verifier;
mod viper;
mod java_exception;
pub mod counterexample;

pub use ast_factory::*;
pub use ast_utils::*;
pub use verification_backend::*;
pub use verification_context::*;
pub use verification_result::*;
pub use verifier::*;
pub use viper::*;
pub use java_exception::*;
pub use counterexample::*;
