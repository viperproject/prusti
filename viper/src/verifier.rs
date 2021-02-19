// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#![cfg_attr(feature = "cargo-clippy", allow(new_ret_no_self))]

use ast_factory::*;
use ast_utils::AstUtils;
use jni::objects::JObject;
use jni::JNIEnv;
use jni_utils::JniUtils;
use std::marker::PhantomData;
use std::path::PathBuf;
use verification_backend::VerificationBackend;
use verification_result::VerificationError;
use verification_result::VerificationResult;
use viper_sys::wrappers::viper::*;
use viper_sys::wrappers::scala;
use silicon_counterexample::SiliconCounterexample;

pub mod state {
    pub struct Uninitialized;
    pub struct Stopped;
    pub struct Started;
}

pub struct Verifier<'a, VerifierState> {
    env: &'a JNIEnv<'a>,
    verifier_wrapper: silver::verifier::Verifier<'a>,
    verifier_instance: JObject<'a>,
    jni: JniUtils<'a>,
    state: PhantomData<VerifierState>,
}

impl<'a, VerifierState> Verifier<'a, VerifierState> {
    pub fn new(
        env: &'a JNIEnv,
        backend: VerificationBackend,
        report_path: Option<PathBuf>,
    ) -> Verifier<'a, state::Uninitialized> {
        let jni = JniUtils::new(env);
        let reporter = if let Some(real_report_path) = report_path {
            jni.unwrap_result(silver::reporter::CSVReporter::with(env).new(
                jni.new_string("csv_reporter"),
                jni.new_string(real_report_path.to_str().unwrap()),
            ))
        } else {
            jni.unwrap_result(silver::reporter::NoopReporter_object::with(env).singleton())
        };
        let utils = JniUtils::new(env);
        let debug_info = utils.new_seq(&[]);
        let verifier_wrapper = silver::verifier::Verifier::with(env);
        let verifier_instance = jni.unwrap_result(match backend {
            VerificationBackend::Silicon => {
                silicon::Silicon::with(env).new(reporter, debug_info)
            }
            VerificationBackend::Carbon => {
                carbon::CarbonVerifier::with(env).new(reporter, debug_info)
            },
        });

        let name = jni.to_string(jni.unwrap_result(verifier_wrapper.call_name(verifier_instance)));
        let build_version =
            jni.to_string(jni.unwrap_result(verifier_wrapper.call_buildVersion(verifier_instance)));
        info!("Using backend {} version {}", name, build_version);

        Verifier {
            env,
            verifier_wrapper,
            verifier_instance,
            jni,
            state: PhantomData,
        }
    }
}

impl<'a> Verifier<'a, state::Uninitialized> {
    pub fn parse_command_line(self, args: &[String]) -> Verifier<'a, state::Stopped> {
        {
            let args = self.jni.new_seq(
                &args
                    .iter()
                    .map(|x| self.jni.new_string(x))
                    .collect::<Vec<JObject>>(),
            );
            self.jni.unwrap_result(
                self.verifier_wrapper
                    .call_parseCommandLine(self.verifier_instance, args),
            );
        }
        Verifier {
            env: self.env,
            verifier_wrapper: self.verifier_wrapper,
            verifier_instance: self.verifier_instance,
            jni: self.jni,
            state: PhantomData,
        }
    }
}

impl<'a> Verifier<'a, state::Stopped> {
    pub fn start(self) -> Verifier<'a, state::Started> {
        self.jni
            .unwrap_result(self.verifier_wrapper.call_start(self.verifier_instance));

        Verifier {
            env: self.env,
            verifier_wrapper: self.verifier_wrapper,
            verifier_instance: self.verifier_instance,
            jni: self.jni,
            state: PhantomData,
        }
    }
}

impl<'a> Verifier<'a, state::Started> {
    pub fn verify(&self, program: Program) -> VerificationResult {
        let ast_utils = AstUtils::new(self.env);

        debug!(
            "Program to be verified:\n{}",
            ast_utils.pretty_print(program)
        );

        run_timed!("Viper consistency checks", debug,
            let consistency_errors = match ast_utils.check_consistency(program) {
                Ok(errors) => errors,
                Err(java_exception) => {
                    return VerificationResult::JavaException(java_exception);
                }
            };
        );

        if !consistency_errors.is_empty() {
            debug!(
                "The provided Viper program has {} consistency errors.",
                consistency_errors.len()
            );
            return VerificationResult::ConsistencyErrors(
                consistency_errors
                    .into_iter()
                    .map(|e| self.jni.to_string(e))
                    .collect(),
            );
        }

        run_timed!("Viper verification", debug,
            let viper_result = self.jni.unwrap_result(
                self.verifier_wrapper
                    .call_verify(self.verifier_instance, program.to_jobject()),
            );
        );
        debug!(
            "Viper verification result: {}",
            self.jni.to_string(viper_result)
        );

        let is_failure = self
            .jni
            .is_instance_of(viper_result, "viper/silver/verifier/Failure");
        
        
        //TODO: understand this and extract counterexample here
        if is_failure {
            let mut errors: Vec<VerificationError> = vec![];

            let viper_errors = self.jni.seq_to_vec(self.jni.unwrap_result(
                silver::verifier::Failure::with(self.env).call_errors(viper_result),
            ));

            let verification_error_wrapper = silver::verifier::VerificationError::with(self.env);

            let has_identifier_wrapper = silver::ast::HasIdentifier::with(self.env);

            let error_reason_wrapper = silver::verifier::ErrorReason::with(self.env);

            for viper_error in viper_errors {
                let is_verification_error = self
                    .jni
                    .is_instance_of(viper_error, "viper/silver/verifier/VerificationError");

                if !is_verification_error {
                    let is_aborted_exceptionally = self
                        .jni
                        .is_instance_of(viper_error, "viper/silver/verifier/AbortedExceptionally");

                    if is_aborted_exceptionally {
                        let exception = self.jni.unwrap_result(
                            silver::verifier::AbortedExceptionally::with(self.env)
                                .call_cause(viper_error),
                        );
                        let stack_trace =
                            self.jni.unwrap_result(self.jni.get_stack_trace(exception));
                        error!(
                            "The verification aborted due to the following exception: {}",
                            stack_trace
                        );
                    } else {
                        error!(
                            "The verifier returned an unhandled error of type {}: {}",
                            self.jni.class_name(viper_error),
                            self.jni.to_string(viper_error)
                        );
                    }
                    unreachable!(
                        "The verifier returned an unknown error of type {}: {}",
                        self.jni.class_name(viper_error),
                        self.jni.to_string(viper_error)
                    );
                };

                let option_original_counterexample = self
                    .jni
                    .unwrap_result(verification_error_wrapper.call_counterexample(viper_error));
                
                let counterexample: Option<SiliconCounterexample> = if !self
                    .jni
                    .is_instance_of(option_original_counterexample, "scala/None$")
                {
                    let original_counterexample = self
                        .jni
                        .unwrap_result(scala::Some::with(self.env).call_get(option_original_counterexample));
                    if self.jni.is_instance_of(original_counterexample, "viper/silicon/interfaces/SiliconMappedCounterexample"){
                        //only the Mapped Counterexamples are processed
                        let ce = SiliconCounterexample::new(self.env, self.jni, original_counterexample);
                        Some(ce)
                    } else {
                        None
                    }
                } else {
                    None
                };
                

                let reason = self
                    .jni
                    .unwrap_result(verification_error_wrapper.call_reason(viper_error));

                let reason_pos = self
                    .jni
                    .unwrap_result(error_reason_wrapper.call_pos(reason));

                let reason_pos_id = if self
                    .jni
                    .is_instance_of(reason_pos, "viper/silver/ast/HasIdentifier")
                {
                    Some(
                        self.jni.get_string(
                            self.jni
                                .unwrap_result(has_identifier_wrapper.call_id(reason_pos)),
                        ),
                    )
                } else {
                    debug!(
                        "The verifier returned an error whose offending node position has no identifier: {:?}",
                        self.jni.to_string(viper_error)
                    );
                    None
                };

                let error_full_id = self.jni.get_string(
                    self.jni
                        .unwrap_result(verification_error_wrapper.call_fullId(viper_error)),
                );

                let pos = self
                    .jni
                    .unwrap_result(verification_error_wrapper.call_pos(viper_error));

                let message =
                    self.jni.to_string(self.jni.unwrap_result(
                        verification_error_wrapper.call_readableMessage(viper_error),
                    ));

                let pos_id =
                    if self
                        .jni
                        .is_instance_of(pos, "viper/silver/ast/HasIdentifier")
                    {
                        Some(self.jni.get_string(
                            self.jni.unwrap_result(has_identifier_wrapper.call_id(pos)),
                        ))
                    } else {
                        debug!(
                            "The verifier returned an error whose position has no identifier: {:?}",
                            self.jni.to_string(viper_error)
                        );
                        None
                    };

                errors.push(VerificationError::new(
                    error_full_id,
                    pos_id,
                    reason_pos_id,
                    message,
                    counterexample,
                ))
            }

            VerificationResult::Failure(errors)
        } else {
            VerificationResult::Success()
        }
    }
}
