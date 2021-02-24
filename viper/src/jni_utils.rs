// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::error_chain::ChainedError;
use jni::errors::{Result as JniResult, ErrorKind};
use jni::objects::JObject;
use jni::objects::JString;
use jni::strings::JNIString;
use jni::sys::jsize;
use jni::JNIEnv;
use viper_sys::wrappers::*;
use crate::java_exception::JavaException;

#[derive(Clone, Copy)]
pub struct JniUtils<'a> {
    env: &'a JNIEnv<'a>,
}

impl<'a> JniUtils<'a> {
    pub fn new(env: &'a JNIEnv) -> Self {
        JniUtils { env }
    }

    /// Generates the stack trace from a Java Exception
    pub fn get_stack_trace(&self, t: JObject<'a>) -> JniResult<String> {
        let sw = java::io::StringWriter::with(self.env).new()?;
        let pw = java::io::PrintWriter::with(self.env).new(sw)?;
        java::lang::Throwable::with(self.env).call_printStackTrace(t, pw)?;
        Ok(self.to_string(sw))
    }

    /// Unwrap a JniResult<T>, retrieving information on raised Java exceptions.
    pub fn unwrap_or_exception<T>(&self, res: JniResult<T>) -> Result<T, JavaException> {
        res.map_err(|error| {
            if let ErrorKind::JavaException = *error.kind() {
                let exception = self.env.exception_occurred().unwrap_or_else(|e| {
                    error!("{}", e.display_chain().to_string());
                    panic!("Failed to get the raised Java exception.");
                });
                self.env.exception_clear().unwrap_or_else(|e| {
                    error!("{}", e.display_chain().to_string());
                    panic!("Failed to clear an exception in the process of being thrown.");
                });
                // Retrieve information on the Java exception.
                // The internal calls `unwrap_result` might lead to infinite recursion.
                let exception_message = self.to_string(*exception);
                let stack_trace = self.unwrap_result(
                    self.get_stack_trace(*exception)
                );
                JavaException::new(exception_message, stack_trace)
            } else {
                // This is not a Java exception
                error!("{}", error.display_chain().to_string());
                panic!("{}", error);
            }
        })
    }

    /// Unwrap a JniResult<T>.
    pub fn unwrap_result<T>(&self, res: JniResult<T>) -> T {
        self.unwrap_or_exception(res).unwrap_or_else(
            |java_exception| panic!("{:?}", java_exception)
        )
    }

    /// Converts a Rust Option<JObject> to a Scala Option
    pub fn new_option(&self, opt: Option<JObject<'a>>) -> JObject<'a> {
        match opt {
            Some(o) => self.unwrap_result(scala::Some::with(self.env).new(o)),
            None => self.unwrap_result(scala::None_object::with(self.env).singleton()),
        }
    }

    /// Converts a Rust String to a Java String
    pub fn new_string<S: Into<JNIString>>(&self, string: S) -> JObject<'a> {
        self.unwrap_result(self.env.new_string(string)).into()
    }

    pub fn new_map(&self, items: &[(JObject<'a>, JObject<'a>)]) -> JObject<'a> {
        let hash_map_wrapper = scala::collection::immutable::HashMap::with(self.env);
        let mut result = self.unwrap_result(hash_map_wrapper.new());
        for &(k, v) in items {
            result = self.unwrap_result(hash_map_wrapper.call_updated(result, k, v));
        }
        result
    }

    /// Converts a Rust number to a Java BigInt
    pub fn new_big_int(&self, number: &dyn ToString) -> JObject {
        let number_string = self.new_string(&number.to_string());

        let java_big_integer =
            self.unwrap_result(java::math::BigInteger::with(self.env).new(number_string));

        self.unwrap_result(scala::math::BigInt::with(self.env).new(java_big_integer))
    }

    /// Converts a Rust Vec<JObject> to a Scala Seq
    pub fn new_seq(&self, objects: &[JObject]) -> JObject {
        let array_buffer_wrapper = scala::collection::mutable::ArrayBuffer::with(self.env);
        let len = objects.len();
        let res = self.unwrap_result(array_buffer_wrapper.new(len as i32));
        for obj in objects.iter() {
            self.unwrap_result(array_buffer_wrapper.call_append(res, *obj));
        }
        self.unwrap_result(array_buffer_wrapper.call_toSeq(res))
    }

    /// Converts a Java String to a Rust String
    pub fn get_string(&self, string_object: JObject) -> String {
        self.unwrap_result(self.env.get_string(JString::from(string_object)))
            .into()
    }

    /// Calls the "toString" method on a Java object and returns the result as a Rust String
    pub fn to_string(&self, object: JObject) -> String {
        let object_wrapper = java::lang::Object::with(self.env);
        let string_object = self.unwrap_result(object_wrapper.call_toString(object));
        self.get_string(string_object)
    }

    /// Returns the name of the class of a Java object as Rust String
    pub fn class_name(&self, object: JObject) -> String {
        let class: JObject = self.unwrap_result(self.env.get_object_class(object)).into();
        let class_name_object =
            self.unwrap_result(java::lang::Class::with(self.env).call_getName(class));
        self.get_string(class_name_object)
    }

    /// Convert a Scala Seq to a Rust Vec<JObject>
    pub fn seq_to_vec(&self, sequence: JObject<'a>) -> Vec<JObject<'a>> {
        let mut res: Vec<JObject> = vec![];
        let seq_wrapper = scala::collection::Seq::with(self.env);
        let length = self.unwrap_result(seq_wrapper.call_length(sequence));
        for i in 0..length {
            let item: JObject = self.unwrap_result(seq_wrapper.call_apply(sequence, i));
            res.push(item);
        }
        res
    }

    /// Checks if an object is a subtype of a Java class
    pub fn is_instance_of(&self, object: JObject, class: &str) -> bool {
        let object_class = self.unwrap_result(self.env.get_object_class(object));
        let super_class = self.unwrap_result(self.env.find_class(class));
        self.unwrap_result(self.env.is_assignable_from(object_class, super_class))
    }

    /// Returns a new Java array of objects, initialised with null values
    pub fn new_object_array(&self, length: jsize) -> JObject {
        let object_class = self.unwrap_result(self.env.find_class("java/lang/Object"));
        JObject::from(self.unwrap_result(self.env.new_object_array(
            length,
            object_class,
            JObject::null(),
        )))
    }

    /// Converts a Scala Seq to a Java Array
    #[allow(dead_code)]
    pub fn seq_to_array(&self, sequence: JObject<'a>, elements_class_ref: &str) -> JObject<'a> {
        let elements_class = self.unwrap_result(self.env.find_class(elements_class_ref));
        let class_tag_object_wrapper = scala::reflect::ClassTag_object::with(self.env);
        let class_tag_object = self.unwrap_result(class_tag_object_wrapper.singleton());
        let elements_class_tag = self.unwrap_result(
            class_tag_object_wrapper.call_apply(class_tag_object, elements_class.into()),
        );
        self.unwrap_result(
            scala::collection::Seq::with(self.env).call_toArray(sequence, elements_class_tag),
        )
    }
}
