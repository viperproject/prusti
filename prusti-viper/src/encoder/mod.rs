// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

pub use self::encoder::Encoder;

mod borrows;
mod builtin_encoder;
mod specs_closures_collector;
mod encoder;
mod errors;
mod foldunfold;
mod initialisation;
mod loop_encoder;
mod mir_encoder;
mod mir_successor;
mod mir_interpreter;
mod name_interner;
mod places;
mod procedure_encoder;
mod pure_function_encoder;
mod spec_encoder;
mod spec_function_encoder;
pub use spec_function_encoder::SpecFunctionKind;
mod stub_function_encoder;
mod stub_procedure_encoder;
mod type_encoder;
mod utils;
mod snapshot;
mod mirror_function_encoder;
mod purifier;
mod array_encoder;
pub mod counterexample;
pub mod counterexample_translation;
