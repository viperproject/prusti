// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use ast_factory::*;
use ast_utils::*;
use jni::AttachGuard;
use std::env;
use std::path::{Path, PathBuf};
use verification_backend::VerificationBackend;
use verifier::state;
use verifier::Verifier;

pub struct VerificationContext<'a> {
    env: AttachGuard<'a>,
}

impl<'a> VerificationContext<'a> {
    pub fn new(env_guard: AttachGuard<'a>) -> Self {
        VerificationContext { env: env_guard }
    }

    pub fn new_ast_factory(&self) -> AstFactory {
        AstFactory::new(&self.env)
    }

    pub fn new_ast_utils(&self) -> AstUtils {
        AstUtils::new(&self.env)
    }

    pub fn new_verifier(&self, backend: VerificationBackend, report_path: Option<PathBuf>) -> Verifier<state::Started> {
        self.new_verifier_with_args(backend, vec![], report_path)
    }

    pub fn new_verifier_with_args(
        &self,
        backend: VerificationBackend,
        extra_args: Vec<String>,
        report_path: Option<PathBuf>,
    ) -> Verifier<state::Started> {
        let mut verifier_args: Vec<String> = vec![];

        // Set Z3 binary
        let z3_exe = env::var("Z3_EXE")
            .expect("the Z3_EXE environment variable should not be empty");
        info!("Using Z3 exe: '{}'", &z3_exe);
        assert!(
            Path::new(&z3_exe).is_file(),
            "The Z3_EXE environment variable ({:?}) does not point to a valid file.",
            z3_exe
        );
        verifier_args.extend(vec!["--z3Exe".to_string(), z3_exe]);

        // Set Boogie binary
        if let VerificationBackend::Carbon = backend {
            let boogie_exe = env::var("BOOGIE_EXE")
                .expect("the BOOGIE_EXE environment variable should not be empty");
            info!("Using BOOGIE exe: '{}'", &boogie_exe);
            assert!(
                Path::new(&boogie_exe).is_file(),
                "The BOOGIE_EXE environment variable ({:?}) does not point to a valid file.",
                boogie_exe
            );
            verifier_args.extend(vec!["--boogieExe".to_string(), boogie_exe]);
        }

        verifier_args.extend(extra_args);
        verifier_args.push("--ignoreFile".to_string());
        verifier_args.push("dummy.vpr".to_string());
        debug!(
            "Verifier arguments: '{}'",
            verifier_args.iter().cloned().collect::<Vec<_>>().join(" ")
        );

        Verifier::<state::Uninitialized>::new(&self.env, backend, report_path)
            .parse_command_line(&verifier_args)
            .start()
    }
}
