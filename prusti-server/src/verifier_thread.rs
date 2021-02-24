// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use super::VerifierRunner;
use futures::channel::oneshot;
use prusti_common::{
    verification_context::VerifierBuilder, verification_service::ViperBackendConfig, vir::Program,
};
use std::{
    sync::{mpsc, Arc, Mutex},
    thread,
};
use viper::VerificationResult;

pub type FutVerificationResult = futures::channel::oneshot::Receiver<VerificationResult>;

struct VerificationRequest {
    pub program: Program,
    pub program_name: String,
    pub sender: oneshot::Sender<VerificationResult>,
}

pub struct VerifierThread {
    pub backend_config: ViperBackendConfig,
    request_sender: Mutex<mpsc::Sender<VerificationRequest>>,
}

impl VerifierThread {
    pub fn new(verifier_builder: Arc<VerifierBuilder>, backend_config: ViperBackendConfig) -> Self {
        let (request_sender, request_receiver) = mpsc::channel::<VerificationRequest>();

        let builder = thread::Builder::new().name(format!(
            "Verifier thread running {}",
            backend_config.backend
        ));

        let config = backend_config.clone();
        builder
            .spawn(move || {
                VerifierRunner::with_runner(&verifier_builder, &config, |runner| {
                    Self::listen_for_requests(runner, request_receiver)
                });
            })
            .unwrap();

        Self {
            backend_config: backend_config,
            request_sender: Mutex::new(request_sender),
        }
    }

    fn listen_for_requests(
        runner: VerifierRunner,
        request_receiver: mpsc::Receiver<VerificationRequest>,
    ) {
        while let Ok(request) = request_receiver.recv() {
            let result = runner.verify(request.program, request.program_name.as_str());
            request.sender.send(result).unwrap_or_else(|err| {
                error!(
                    "verifier thread attempting to send result to dropped receiver: {:?}",
                    err
                );
            });
        }
    }

    pub fn verify(&self, program: Program, program_name: String) -> FutVerificationResult {
        let (tx, rx) = oneshot::channel();
        self.request_sender
            .lock()
            .unwrap()
            .send(VerificationRequest {
                program,
                program_name,
                sender: tx,
            })
            .unwrap();
        rx
    }
}
