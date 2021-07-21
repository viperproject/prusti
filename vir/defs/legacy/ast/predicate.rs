// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::legacy::ast::*;
use std::fmt;
use std::collections::HashSet;

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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EnumVariantIndex(String);
pub type MaybeEnumVariantIndex = Option<EnumVariantIndex>;

impl EnumVariantIndex {
    pub fn get_variant_name(&self) -> &str {
        &self.0
    }

    pub fn new(s: String) -> Self {
        EnumVariantIndex(s)
    }
}

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
