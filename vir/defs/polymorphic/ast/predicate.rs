// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::polymorphic::ast::*;
use std::fmt;
use std::collections::HashSet;

use super::super::super::legacy;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Predicate {
    Struct(StructPredicate),
    Enum(EnumPredicate),
    Bodyless(String, LocalVar),
}

impl fmt::Display for Predicate {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Predicate::Struct(p) => write!(f, "{}", p),
            Predicate::Enum(p) => write!(f, "{}", p),
            Predicate::Bodyless(name, this) => write!(f, "bodyless_predicate {}({});", name, this),
        }
    }
}

impl From<Predicate> for legacy::Predicate {
    fn from(predicate: Predicate) -> legacy::Predicate {
        match predicate {
            Predicate::Struct(struct_predicate) => legacy::Predicate::Struct(legacy::StructPredicate::from(struct_predicate.clone())),
            Predicate::Enum(enum_predicate) => legacy::Predicate::Enum(legacy::EnumPredicate::from(enum_predicate.clone())),
            Predicate::Bodyless(label, local_var) => legacy::Predicate::Bodyless(label.clone(), legacy::LocalVar::from(local_var.clone())),
        }
    }
}

/// The predicate for types that have exactly one variant.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StructPredicate {
    /// The predicate name in Viper.
    pub name: String,
    /// The self reference.
    pub this: LocalVar,
    /// The optional body of the predicate.
    pub body: Option<Expr>,
}

impl fmt::Display for StructPredicate {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "struct_predicate {}({}", self.name, self.this)?;
        match self.body {
            None => writeln!(f, ");"),
            Some(ref body) => {
                writeln!(f, "){{")?;
                writeln!(f, "  {}", body)?;
                writeln!(f, "}}")
            }
        }
    }
}

impl From<StructPredicate> for legacy::StructPredicate {
    fn from(struct_predicate: StructPredicate) -> legacy::StructPredicate {
        legacy::StructPredicate {
            name: struct_predicate.name.clone(),
            this: legacy::LocalVar::from(struct_predicate.this.clone()),
            body: match struct_predicate.body {
                Some(body_expr) => Some(legacy::Expr::from(body_expr.clone())),
                _ => None,
            },
        }
    }
}

/// The predicate for types that have 0 or more than one variants.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EnumPredicate {
    /// The predicate name in Viper.
    pub name: String,
    /// The self reference.
    pub this: LocalVar,
    /// The discriminant field.
    pub discriminant_field: Field,
    /// The restrictions of the discriminant field.
    pub discriminant_bounds: Expr,
    /// `(guard, variant_name, variant_predicate)` of the enum. `guard`
    /// is a condition on `discriminant` under which this variant holds.
    pub variants: Vec<(Expr, String, StructPredicate)>,
}

impl From<EnumPredicate> for legacy::EnumPredicate {
    fn from(enum_predicate: EnumPredicate) -> legacy::EnumPredicate {
        legacy::EnumPredicate {
            name: enum_predicate.name.clone(),
            this: legacy::LocalVar::from(enum_predicate.this.clone()),
            discriminant_field: legacy::Field::from(enum_predicate.discriminant_field.clone()),
            discriminant_bounds: legacy::Expr::from(enum_predicate.discriminant_bounds.clone()),
            variants: enum_predicate.variants.iter().map(|(expr, label, struct_predicate)| (legacy::Expr::from(expr.clone()), label.clone(), legacy::StructPredicate::from(struct_predicate.clone()))).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EnumVariantIndex(String);

impl EnumVariantIndex {
    pub fn get_variant_name(&self) -> &str {
        &self.0
    }

    pub fn new(s: String) -> Self {
        EnumVariantIndex(s)
    }
}

impl From<EnumVariantIndex> for legacy::EnumVariantIndex {
    fn from(enum_variant_index: EnumVariantIndex) -> legacy::EnumVariantIndex {
        legacy::EnumVariantIndex::new(enum_variant_index.0.clone())
    }
}

pub type MaybeEnumVariantIndex = Option<EnumVariantIndex>;

// impl From<Option<EnumVariantIndex>> for Option<legacy::EnumVariantIndex> {
//     fn from(maybe_enum_variant_index: Option<EnumVariantIndex>) -> legacy::Option<EnumVariantIndex> {
//         match maybe_enum_variant_index {
//             Some(enum_variant_index) => legacy::EnumVariantIndex::from(enum_variant_index),
//             _ => None,
//         }
//     }
// }

impl fmt::Display for EnumPredicate {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "enum_predicate {}({}){{\n", self.name, self.this)?;
        write!(f, "  discriminant_field={}\n", self.discriminant_field)?;
        for (guard, name, variant) in self.variants.iter() {
            writeln!(f, "  {}: {} ==> {}\n", name, guard, variant)?;
        }
        writeln!(f, "}}")
    }
}

impl fmt::Display for EnumVariantIndex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{{{}}}", self.0)
    }
}
