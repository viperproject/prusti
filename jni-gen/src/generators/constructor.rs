// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::class_name::*;
use crate::errors::*;
use crate::jni::objects::JValue;
use crate::jni::JNIEnv;
use std::collections::HashMap;
use crate::utils::*;

pub fn generate_constructor(
    env: &JNIEnv,
    class: &ClassName,
    target_signature: Option<String>,
    suffix: Option<String>,
) -> Result<String> {
    let clazz = env.find_class(class.path())?;

    let constructors = env
        .call_method(
            clazz,
            "getConstructors",
            "()[Ljava/lang/reflect/Constructor;",
            &[],
        )?
        .l()?;
    let num_constructors = env.get_array_length(constructors.into_inner())?;

    let mut indexed_constructors = HashMap::new();

    for constructor_index in 0..num_constructors {
        let constructor =
            env.get_object_array_element(constructors.into_inner(), constructor_index)?;

        let constructor_signature = java_str_to_string(
            &env.get_string(
                env.call_static_method(
                    "org/objectweb/asm/Type",
                    "getConstructorDescriptor",
                    "(Ljava/lang/reflect/Constructor;)Ljava/lang/String;",
                    &[JValue::Object(constructor)],
                )?
                .l()?
                .into(),
            )?,
        )?;

        indexed_constructors.insert(constructor_signature, constructor);
    }

    let (constructor_signature, constructor) = match target_signature {
        None => {
            if indexed_constructors.is_empty() {
                return Err(ErrorKind::NoConstructors(class.full_name()).into());
            }
            if indexed_constructors.len() > 1 {
                return Err(ErrorKind::AmbiguousConstructor(
                    class.full_name(),
                    indexed_constructors.keys().map(|k| k.to_string()).collect(),
                )
                .into());
            }
            indexed_constructors.drain().take(1).next().unwrap()
        }
        Some(sign) => match indexed_constructors.get(&sign) {
            Some(constr) => (sign, *constr),
            None => return Err(ErrorKind::NoMatchingConstructor(class.full_name(), sign).into()),
        },
    };

    let mut parameter_names: Vec<String> = vec![];
    let mut parameter_signatures: Vec<String> = vec![];

    let parameters = env
        .call_method(
            constructor,
            "getParameters",
            "()[Ljava/lang/reflect/Parameter;",
            &[],
        )?
        .l()?;
    let num_parameters = env.get_array_length(parameters.into_inner())?;

    for parameter_index in 0..num_parameters {
        let parameter = env.get_object_array_element(parameters.into_inner(), parameter_index)?;
        let parameter_name = env.get_string(
            env.call_method(parameter, "getName", "()Ljava/lang/String;", &[])?
                .l()?
                .into(),
        )?;
        let parameter_type = env
            .call_method(parameter, "getType", "()Ljava/lang/Class;", &[])?
            .l()?;
        let parameter_signature = env.get_string(
            env.call_static_method(
                "org/objectweb/asm/Type",
                "getDescriptor",
                "(Ljava/lang/Class;)Ljava/lang/String;",
                &[JValue::Object(parameter_type)],
            )?
            .l()?
            .into(),
        )?;

        parameter_names.push(java_str_to_valid_rust_argument_name(&parameter_name)?);
        parameter_signatures.push(java_str_to_string(&parameter_signature)?);
    }

    let constructor_name = match suffix {
        None => "new".to_string(),
        Some(s) => format!("new_{}", s),
    };

    Ok(generate(
        class,
        &constructor_name,
        &constructor_signature,
        &parameter_names,
        &parameter_signatures,
    ))
}

fn generate(
    class: &ClassName,
    constructor_name: &str,
    constructor_signature: &str,
    parameter_names: &[String],
    parameter_signatures: &[String],
) -> String {
    assert_eq!(parameter_names.len(), parameter_signatures.len());

    let mut code: Vec<String> = vec![];
    code.push(format!(
        "/// Calls a constructor of Java class `{}`.",
        class.full_name()
    ));
    code.push("///".to_string());
    code.push("/// Type and Java signature of parameters:".to_string());

    for i in 0..parameter_names.len() {
        let par_name = &parameter_names[i];
        let par_sign = &parameter_signatures[i];
        let par_type = generate_jni_type(par_sign);
        code.push(format!(
            "/// - `{}`: `{}` (`{}`)",
            par_name, par_type, par_sign
        ));
    }

    code.push("///".to_string());
    code.push(format!(
        "/// Return type and Java signature: `JObject` (`L{};`)",
        class.path()
    ));

    code.push(format!("pub fn {}(", constructor_name));
    code.push("    &self,".to_string());

    for i in 0..parameter_names.len() {
        let par_name = &parameter_names[i];
        let par_sign = &parameter_signatures[i];
        let par_type = generate_jni_type(par_sign);
        code.push(format!("    {}: {},", par_name, par_type));
    }

    code.push(") -> JNIResult<JObject<'a>> {".to_string());

    code.push(format!("    let class = self.env.find_class(\"{}\")?;", class.path()));

    code.push(format!(
        "    let method_signature = \"{}\";",
        constructor_signature
    ));

    code.push("    let method_id = self.env.get_method_id(".to_string());
    code.push("        class,".to_string());
    code.push("        \"<init>\",".to_string());
    code.push("        method_signature".to_string());
    code.push("    )?;".to_string());

    code.push("    let result = self.env.new_object_unchecked(".to_string());
    code.push("        class,".to_string());
    code.push("        method_id,".to_string());
    code.push("        &[".to_string());

    for i in 0..parameter_names.len() {
        let par_name = &parameter_names[i];
        let par_sign = &parameter_signatures[i];
        let par_jvalue = generate_jvalue_wrapper(par_name, par_sign);
        code.push(format!("            {},", par_jvalue));
    }

    code.push("        ]".to_string());
    code.push("    );".to_string());

    code.push("    self.env.delete_local_ref(class.into()).unwrap();".to_string());
    code.push("    result".to_string());
    code.push("}".to_string());

    code.join("\n") + "\n"
}
