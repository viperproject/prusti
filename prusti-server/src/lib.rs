// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![deny(unused_must_use)]

extern crate reqwest;
extern crate viper;
extern crate warp;
#[macro_use]
extern crate log;
extern crate bincode;
extern crate futures;
extern crate num_cpus;
extern crate prusti_common;
extern crate tokio;
#[macro_use]
extern crate serde;

mod service;
mod verifier_runner;
mod verifier_thread;

use futures::Future;
use prusti_common::{verification_context::VerifierBuilder, verification_service::*, Stopwatch};
pub use service::*;
use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
};
pub use verifier_runner::*;
use verifier_thread::*;
use viper::ProgramVerificationResult;

#[derive(Debug, Serialize, Deserialize)]
pub struct VerifierPanicked;
pub type RemoteVerificationResult = Result<ProgramVerificationResult, VerifierPanicked>;

pub struct PrustiServer {
    verifier_builder: Arc<VerifierBuilder>,
    threads: RwLock<VecDeque<VerifierThread>>,
    cache_size: usize,
}

impl PrustiServer {
    pub fn new(cache_size: usize) -> PrustiServer {
        let stopwatch = Stopwatch::start("prusti-server", "JVM startup");
        let verifier_builder = Arc::new(VerifierBuilder::new());
        stopwatch.finish();

        PrustiServer {
            verifier_builder,
            threads: RwLock::new(VecDeque::with_capacity(cache_size)),
            cache_size,
        }
    }

    pub fn run_verifier(&self, request: VerificationRequest) -> RemoteVerificationResult {
        // try to find and take out an existing threads from our cache
        let existing_thread = {
            let mut threads = self.threads.write().unwrap();
            let index = threads
                .iter()
                .position(|thread| thread.backend_config == request.backend_config);
            index.map(|index| threads.remove(index).unwrap())
        };

        // if no thread found, create a new one
        let thread = existing_thread.unwrap_or_else(|| {
            VerifierThread::new(
                self.verifier_builder.clone(),
                request.backend_config.clone(),
            )
        });

        match thread
            .verify(request.programs, request.program_name.clone())
            .wait()
        {
            Ok(result) => {
                // put back the thread for later reuse
                let mut threads = self.threads.write().unwrap();
                if threads.len() >= self.cache_size {
                    // evict least-recently-used thread from cache)
                    threads.pop_back();
                }
                threads.push_front(thread);
                Ok(result)
            }
            Err(_) => {
                // canceled—the verifier thread panicked
                error!(
                    "Panic while handling verification request {}",
                    request.program_name
                );
                Err(VerifierPanicked)
            }
        }
    }
}
