// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

#[macro_use]
mod macros;
mod ast_type;
mod expression;
mod position;
mod program;
mod statement;
mod structs;

use jni::objects::JObject;
use jni::JNIEnv;
use crate::jni_utils::JniUtils;
use viper_sys::wrappers::viper::silver::ast;

pub use self::ast_type::*;
pub use self::expression::*;
pub use self::position::*;
pub use self::program::*;
pub use self::statement::*;
pub use self::structs::*;

#[derive(Clone, Copy)]
pub struct AstFactory<'a> {
    env: &'a JNIEnv<'a>,
    jni: JniUtils<'a>,
}

impl<'a> AstFactory<'a> {
    pub fn new(env: &'a JNIEnv) -> Self {
        let jni = JniUtils::new(env);
        AstFactory { env, jni }
    }

    // === Info ===

    fn no_info(&self) -> JObject {
        self.jni
            .unwrap_result(ast::NoInfo_object::with(self.env).singleton())
    }

    fn simple_info(&self, comments: &[&str]) -> JObject {
        self.jni.unwrap_result(
            ast::SimpleInfo::with(self.env).new(
                self.jni.new_seq(
                    &comments
                        .iter()
                        .map(|x| self.jni.new_string(x))
                        .collect::<Vec<JObject>>(),
                ),
            ),
        )
    }

    fn no_trafos(&self) -> JObject {
        self.jni
            .unwrap_result(ast::NoTrafos_object::with(self.env).singleton())
    }
}
