// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::polymorphic::ast::*;
use std::fmt;

use super::super::super::legacy;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Trigger(Vec<Expr>);

impl fmt::Display for Trigger {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{{{}}}",
            self.0
                .iter()
                .map(|x| x.to_string())
                .collect::<Vec<String>>()
                .join(", ")
        )
    }
}

impl Trigger {
    pub fn new(items: Vec<Expr>) -> Self {
        Trigger(items)
    }
}

impl From<Trigger> for legacy::Trigger {
    fn from(trigger: Trigger) -> legacy::Trigger {
        legacy::Trigger::new(trigger.0.iter().map(|expr| legacy::Expr::from(expr.clone())).collect())
    }
}