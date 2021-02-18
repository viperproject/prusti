// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::ast_factory::structs::Position;
use crate::ast_factory::AstFactory;
use jni::strings::JNIString;
use jni::sys::jint;
use viper_sys::wrappers::java;
use viper_sys::wrappers::viper::silver::ast;

impl<'a> AstFactory<'a> {
    pub fn no_position(&self) -> Position<'a> {
        let obj = self
            .jni
            .unwrap_result(ast::NoPosition_object::with(self.env).singleton());
        Position::new(obj)
    }

    pub fn line_column_position(&self, line: jint, column: jint) -> Position<'a> {
        let obj = self
            .jni
            .unwrap_result(ast::LineColumnPosition::with(self.env).new(line, column));
        Position::new(obj)
    }

    pub fn identifier_position<S: Into<JNIString>>(
        &self,
        line: jint,
        column: jint,
        pos_id: S,
    ) -> Position<'a> {
        let obj = self.jni.unwrap_result(
            ast::IdentifierPosition::with(self.env).new(
                self.jni.unwrap_result(
                    java::nio::file::Paths::with(self.env)
                        .call_get(self.jni.new_string(""), self.jni.new_object_array(0)),
                ),
                self.line_column_position(line, column).to_jobject(),
                self.jni.new_option(None),
                self.jni.new_string(pos_id),
            ),
        );
        Position::new(obj)
    }
}
