// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use JavaException;
use counterexample::Counterexample;

/// The result of a verification request on a Viper program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VerificationResult {
    /// The program verified.
    Success(),
    /// The program did not verify.
    Failure(Vec<VerificationError>),
    /// The program has consistency errors.
    ConsistencyErrors(Vec<String>),
    /// The verification raised a Java exception.
    JavaException(JavaException),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VerificationError {
    pub full_id: String,
    pub pos_id: Option<String>,
    pub reason_pos_id: Option<String>,
    pub message: String,
    pub counterexample: Option<Counterexample>
}

impl VerificationError {
    pub fn new(
        full_id: String,
        pos_id: Option<String>,
        reason_pos_id: Option<String>,
        message: String,
        counterexample: Option<Counterexample>
    ) -> Self {
        VerificationError {
            full_id,
            pos_id,
            reason_pos_id,
            message,
            counterexample
        }
    }
}
