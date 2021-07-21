// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use jni::*;
use jni_utils::JniUtils;
use std::env;
use std::fs;
use verification_context::*;
use viper_sys::wrappers::*;
use std::path::Path;

pub struct Viper {
    jvm: JavaVM,
}

impl Default for Viper {
    fn default() -> Self {
        Self::new()
    }
}

impl Viper {
    pub fn new() -> Self {
        Self::new_with_args(vec![])
    }

    pub fn new_with_args(java_args: Vec<String>) -> Self {
        let viper_home = env::var("VIPER_HOME")
            .expect("the VIPER_HOME environment variable should not be empty");
        let heap_size = env::var("JAVA_HEAP_SIZE").unwrap_or_else(|_| "2048".to_string());

        debug!("Using Viper home: '{}'", &viper_home);
        assert!(
            Path::new(&viper_home).is_dir(),
            "The VIPER_HOME environment variable ({:?}) does not point to a valid folder.",
            viper_home
        );

        let jar_paths: Vec<String> = fs::read_dir(&viper_home)
            .expect(&format!("failed to open {:?}", viper_home))
            .map(|x| x.unwrap().path().to_str().unwrap().to_string())
            .collect();

        debug!("Java classpath: {}", jar_paths.clone().join(":"));

        let classpath_separator = if cfg!(windows) { ";" } else { ":" };
        let init_args = InitArgsBuilder::new()
            .version(JNIVersion::V8)
            .option(&format!("-Djava.class.path={}", jar_paths.join(classpath_separator)))
            // maximum heap size
            .option(&format!("-Xmx{}m", heap_size))
            // stack size
            .option("-Xss1024m");
            //.option("-Xdebug")
            //.option("-verbose:gc")
            //.option("-Xcheck:jni")
            //.option("-XX:+CheckJNICalls")
            //.option("-Djava.security.debug=all")
            //.option("-verbose:jni")
            //.option("-XX:+TraceJNICalls")
        let jvm_args = java_args
            .into_iter()
            .fold(init_args, |curr_args, opt| curr_args.option(&opt))
            .build()
            .unwrap_or_else(|e| {
                panic!("{} source: {:?}", e, std::error::Error::source(&e));
            });

        let jvm = JavaVM::new(jvm_args).unwrap_or_else(|e| {
            panic!("{} source: {:?}", e, std::error::Error::source(&e));
        });

        // Log JVM and Java version
        {
            let env = jvm
                .attach_current_thread()
                .expect("failed to attach jvm thread");

            let jni = JniUtils::new(&env);

            let system_wrapper = java::lang::System::with(&env);

            let vm_name = jni.to_string(
                jni.unwrap_result(system_wrapper.call_getProperty(jni.new_string("java.vm.name"))),
            );

            let java_version = jni.to_string(
                jni.unwrap_result(system_wrapper.call_getProperty(jni.new_string("java.version"))),
            );

            info!("Using JVM {}, Java {}", vm_name, java_version);
        }

        let this = Viper { jvm };

        this
    }

    pub fn new_verification_context(&self) -> VerificationContext {
        let env_guard = self
            .jvm
            .attach_current_thread()
            .expect("failed to attach jvm thread");

        VerificationContext::new(env_guard)
    }
}
