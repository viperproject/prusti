// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::errors::*;
use jni::strings::JavaStr;
use std::ffi::CStr;

pub fn get_return_signature(signature: &str) -> String {
    let splitted: Vec<&str> = signature.split(')').collect();
    splitted[1].to_string()
}

pub fn generate_jni_type(signature: &str) -> String {
    match signature.chars().nth(0).unwrap() {
        '[' | 'L' => "JObject".to_string(),
        'B' => "jbyte".to_string(),
        'C' => "jchar".to_string(),
        'S' => "jshort".to_string(),
        'I' => "jint".to_string(),
        'J' => "jlong".to_string(),
        'Z' => "bool".to_string(),
        'F' => "jfloat".to_string(),
        'D' => "jdouble".to_string(),
        'V' => "()".to_string(),
        x => unreachable!(x),
    }
}

pub fn generate_return_jni_type(signature: &str) -> String {
    match signature.chars().nth(0).unwrap() {
        '[' | 'L' => "JObject<'a>".to_string(),
        'B' => "jbyte".to_string(),
        'C' => "jchar".to_string(),
        'S' => "jshort".to_string(),
        'I' => "jint".to_string(),
        'J' => "jlong".to_string(),
        'Z' => "bool".to_string(),
        'F' => "jfloat".to_string(),
        'D' => "jdouble".to_string(),
        'V' => "()".to_string(),
        x => unreachable!(x),
    }
}

pub fn generate_jni_type_char(signature: &str) -> String {
    match signature.chars().nth(0).unwrap() {
        '[' => "l".to_string(),
        x => x.to_lowercase().to_string(),
    }
}

pub fn generate_jvalue_wrapper(par: &str, signature: &str) -> String {
    match signature.chars().nth(0).unwrap() {
        '[' | 'L' => format!("JValue::Object({})", par),
        'B' => format!("JValue::Byte({})", par),
        'C' => format!("JValue::Char({})", par),
        'S' => format!("JValue::Short({})", par),
        'I' => format!("JValue::Int({})", par),
        'J' => format!("JValue::Long({})", par),
        'Z' => format!("JValue::Bool({} as jboolean)", par),
        'F' => format!("JValue::Float({})", par),
        'D' => format!("JValue::Double({})", par),
        x => unreachable!(x),
    }
}

pub fn java_str_to_string(string: &JavaStr) -> Result<String> {
    unsafe { Ok(CStr::from_ptr(string.get_raw()).to_str()?.to_string()) }
}

pub fn java_str_to_valid_rust_argument_name(string: &JavaStr) -> Result<String> {
    let mut res = "arg_".to_string();
    res.push_str(
        &java_str_to_string(string)?
            .replace("_", "___")
            .replace("$", "_d_"),
    );
    Ok(res)
}
