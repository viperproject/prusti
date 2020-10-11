// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

pub use self::encoder::Encoder;

mod borrows;
mod builtin_encoder;
mod encoder;
mod errors;
mod expiration_tool;
mod expires_first_domain;
mod foldunfold;
mod initialisation;
mod interface_reborrowing_graph;
mod loop_encoder;
mod mir_encoder;
mod mir_successor;
mod mir_interpreter;
mod optimizer;
mod places;
mod procedure_encoder;
mod pure_function_encoder;
mod snapshot_encoder;
mod snapshot_spec_patcher;
mod spec_encoder;
mod stub_function_encoder;
mod stub_procedure_encoder;
mod type_encoder;
mod utils;
