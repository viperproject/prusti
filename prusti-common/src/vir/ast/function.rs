// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::vir::ast::*;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    pub formal_args: Vec<LocalVar>,
    pub return_type: Type,
    pub pres: Vec<Expr>,
    pub posts: Vec<Expr>,
    pub body: Option<Expr>,
}

impl fmt::Display for Function {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "function {}(", self.name)?;
        let mut first = true;
        for arg in &self.formal_args {
            if !first {
                write!(f, ", ")?;
            }
            write!(f, "{:?}", arg)?;
            first = false
        }
        writeln!(f, "): {}", self.return_type)?;
        for pre in &self.pres {
            writeln!(f, "  requires {}", pre)?;
        }
        for post in &self.posts {
            writeln!(f, "  ensures {}", post)?;
        }
        if let Some(ref body) = self.body {
            writeln!(f, "{{")?;
            writeln!(f, "\t{}", body)?;
            write!(f, "}}")?;
        }
        write!(f, "")
    }
}

impl Function {
    pub fn inline_body(&self, args: Vec<Expr>) -> Expr {
        let subst: HashMap<LocalVar, Expr> = self
            .formal_args
            .iter()
            .cloned()
            .zip(args.into_iter())
            .collect();
        // TODO: this does not handle let expressions, quantifiers, and so on
        self.body.clone().unwrap().fold_expr(|orig_expr| {
            if let Expr::Local(ref local, ref _pos) = orig_expr {
                subst[local].clone()
            } else {
                orig_expr
            }
        })
    }

    pub fn apply(&self, args: Vec<Expr>) -> Expr {
        Expr::FuncApp(
            self.name.to_string(),
            args,
            self.formal_args.clone(),
            self.return_type.clone(),
            Position::default(),
        )
    }
}

pub fn compute_identifier(name: &str, formal_args: &[LocalVar], return_type: &Type) -> String {
    let mut identifier = name.to_string();
    // Include the signature of the function in the function name
    identifier.push_str("__$TY$__");
    fn type_name(typ: &Type) -> String {
        match typ {
            Type::Int => "$int$".to_string(),
            Type::Bool => "$bool$".to_string(),
            Type::TypedRef(ref name) => name.to_string(),
            Type::Domain(ref name) => name.to_string(),
            Type::Snapshot(ref name) => format!("Snap${}", name),
            Type::Seq(ref elem_ty) => format!("Seq${}", type_name(elem_ty)),
            Type::Float(t) => match t {
                &FloatSize::F32 => "$float32$".to_string(),
                &FloatSize::F64 => "$float64$".to_string(),
            }
        }
    }
    for arg in formal_args {
        identifier.push_str(&type_name(&arg.typ));
        identifier.push_str("$");
    }
    identifier.push_str(&type_name(return_type));
    identifier
}

impl WithIdentifier for Function {
    fn get_identifier(&self) -> String {
        compute_identifier(&self.name, &self.formal_args, &self.return_type)
    }
}
