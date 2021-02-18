// © 2020, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::config;
use std::{fs::create_dir_all, path::PathBuf};
use crate::verification_service::ViperBackendConfig;
use viper::{self, AstFactory, VerificationBackend, Viper};

/// A verifier builder is an object that lives entire program's
/// lifetime, has no mutable state, and is responsible for constructing
/// verification context instances. The user of this interface is supposed
/// to create a new verifier for each crate he or she wants to verify.
/// The main motivation for having a builder is to be able to cache the JVM
/// initialization.
pub struct VerifierBuilder {
    viper: Viper,
}

impl VerifierBuilder {
    pub fn new() -> Self {
        Self::new_with_backend(VerificationBackend::from_str(&config::viper_backend()))
    }

    pub fn new_with_backend(backend: VerificationBackend) -> Self {
        Self {
            viper: Viper::new_with_args(config::extra_jvm_args(), backend),
        }
    }

    pub fn new_verification_context(&self) -> VerificationContext {
        let verification_ctx = self.viper.new_verification_context();
        VerificationContext::new(verification_ctx)
    }
}

impl Default for VerifierBuilder {
    fn default() -> Self {
        VerifierBuilder::new()
    }
}

/// A verification context is an object that lives entire verification's lifetime.
/// Its main purpose is to build verifiers.
/// The main motivation for having a verification context is to be able to detach the current
/// thread from the JVM when the verification context goes out of scope.
pub struct VerificationContext<'v> {
    verification_ctx: viper::VerificationContext<'v>,
}

impl<'v> VerificationContext<'v> {
    fn new(verification_ctx: viper::VerificationContext<'v>) -> Self {
        VerificationContext { verification_ctx }
    }

    pub fn new_viper_verifier(
        &self,
        backend_config: &ViperBackendConfig,
    ) -> viper::Verifier<viper::state::Started> {
        let mut verifier_args: Vec<String> = backend_config.verifier_args.clone();
        let log_path: PathBuf = PathBuf::from(config::log_dir()).join("viper_tmp");
        create_dir_all(&log_path).unwrap();
        let report_path: PathBuf = log_path.join("report.csv");
        let log_dir_str = log_path.to_str().unwrap();
        match backend_config.backend {
            VerificationBackend::Silicon => verifier_args.extend(vec![
                "--tempDirectory".to_string(),
                log_dir_str.to_string(),
            ]),
            VerificationBackend::Carbon => verifier_args.extend(vec![
                "--boogieOpt".to_string(),
                format!("/logPrefix {}", log_dir_str),
            ]),
        }
        if config::dump_debug_info() {
            match backend_config.backend {
                VerificationBackend::Silicon => verifier_args.extend(vec![
                    "--printMethodCFGs".to_string(),
                    //"--printTranslatedProgram".to_string(),
                ]),
                VerificationBackend::Carbon => verifier_args.extend::<Vec<_>>(vec![
                    //"--print".to_string(), "./log/boogie_program/program.bpl".to_string(),
                ]),
            }
        }

        self.verification_ctx.new_verifier_with_args(
            backend_config.backend,
            verifier_args,
            Some(report_path),
        )
    }

    pub fn new_ast_factory(&self) -> AstFactory {
        self.verification_ctx.new_ast_factory()
    }

    pub fn new_ast_utils(&self) -> viper::AstUtils {
        self.verification_ctx.new_ast_utils()
    }
}
