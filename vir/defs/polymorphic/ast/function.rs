// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::legacy::ast::*;
use std::collections::HashMap;
use std::fmt;

use super::super::super::legacy;

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

impl From<Function> for legacy::Function {
    fn from(function: Function) -> legacy::Function {
        legacy::Function {
            name: function.name.clone(),
            formal_args: function.formal_args.iter().map(|formal_arg| legacy::LocalVar::from(formal_arg.clone())).collect(),
            return_type: legacy::Type::from(function.return_type.clone()),
            pres: function.pres.iter().map(|pre| legacy::Expr::from(pre.clone())).collect(),
            posts: function.posts.iter().map(|post| legacy::Expr::from(post.clone())).collect(),
            body: match function.body {
                Some(body_expr) => Some(legacy::Expr::from(body_expr.clone())),
                _ => None,
            }
        }
    }
}