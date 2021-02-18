// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::class_name::*;
use crate::errors::*;
use crate::generators::constructor::*;
use crate::generators::method::*;
use crate::generators::scala_object_getter::*;
use jni::JNIEnv;
use crate::wrapper_spec::*;

pub struct ClassGenerator<'a> {
    env: &'a JNIEnv<'a>,
    class: ClassName,
    items: Vec<ItemWrapperSpec>,
}

impl<'a> ClassGenerator<'a> {
    pub fn new(env: &'a JNIEnv<'a>, class: ClassName, items: Vec<ItemWrapperSpec>) -> Self {
        ClassGenerator { env, class, items }
    }

    fn check(&self) -> Result<()> {
        self.env
            .find_class(self.class.path())
            .map(|_| ())
            .chain_err(|| ErrorKind::NoClass(self.class.full_name()))
    }

    pub fn generate(&self) -> Result<String> {
        self.check()?;

        Ok(vec![
            format!(
                "//! Automatically generated code for '{}'\n",
                self.class.full_name()
            ),
            "#![allow(dead_code)]\n".to_string(),
            "#![allow(non_snake_case)]\n".to_string(),
            "#![allow(unused_imports)]\n".to_string(),
            "#![cfg_attr(feature = \"cargo-clippy\", allow(new_ret_no_self))]\n".to_string(),
            "#![cfg_attr(feature = \"cargo-clippy\", allow(wrong_self_convention))]\n".to_string(),
            "#![cfg_attr(feature = \"cargo-clippy\", allow(too_many_arguments))]\n".to_string(),
            self.generate_imports(),
            self.generate_struct(),
            self.generate_begin_impl(),
            self.generate_items()?,
            self.generate_end_impl(),
        ]
        .join("\n"))
    }

    fn generate_imports(&self) -> String {
        vec![
            "use jni::JNIEnv;",
            "use jni::objects::JObject;",
            "use jni::objects::JValue;",
            "use jni::objects::JClass;",
            "use jni::errors::Result as JNIResult;",
            "use jni::sys::*;",
            "use jni::signature::*;",
            "use std::str::FromStr;",
        ]
        .join("\n")
            + "\n"
    }

    fn generate_struct(&self) -> String {
        vec![
            "#[allow(non_camel_case_types)]".to_string(),
            format!("pub struct {}<'a> {{", self.class.rust_name()),
            "    env: &'a JNIEnv<'a>,".to_string(),
            "}".to_string(),
        ]
        .join("\n")
            + "\n"
    }

    fn generate_begin_impl(&self) -> String {
        vec![
            format!("impl<'a> {}<'a> {{", self.class.rust_name()),
            "".to_string(),
            "pub fn with(env: &'a JNIEnv<'a>) -> Self {".to_string(),
            format!("    {}{{ env: env }}", self.class.rust_name()),
            "}".to_string(),
        ]
        .join("\n")
            + "\n"
    }

    fn generate_end_impl(&self) -> String {
        format!("}} // end of impl {}\n", self.class.rust_name())
    }

    fn generate_items(&self) -> Result<String> {
        let mut gen_items: Vec<String> = vec![];

        if self.items.is_empty() {
            warn!("Class {} does not wrap any item", self.class.full_name())
        }

        for item in &self.items {
            let gen = match *item {
                ItemWrapperSpec::ScalaObjectGetter() => {
                    generate_scala_object_getter(self.env, &self.class)?
                }
                ItemWrapperSpec::Constructor {
                    ref signature,
                    ref suffix,
                } => {
                    generate_constructor(self.env, &self.class, signature.clone(), suffix.clone())?
                }
                ItemWrapperSpec::Method {
                    ref name,
                    ref signature,
                    ref suffix,
                } => generate_method(
                    self.env,
                    &self.class,
                    name,
                    signature.clone(),
                    suffix.clone(),
                )?,
            };
            gen_items.push(gen)
        }
        Ok(gen_items.join("\n") + "\n")
    }
}
