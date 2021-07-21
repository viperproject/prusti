// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use std::{
    cmp::Ordering,
    collections::HashMap,
    fmt,
    hash::{Hash, Hasher},
    mem::discriminant,
    ops,
};

use super::super::super::legacy;

/// The identifier of a statement. Used in error reporting.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    line: i32,
    column: i32,
    id: u64,
}

impl Position {
    pub fn new(line: i32, column: i32, id: u64) -> Self {
        Position { line, column, id }
    }

    pub fn line(&self) -> i32 {
        self.line
    }

    pub fn column(&self) -> i32 {
        self.column
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn is_default(&self) -> bool {
        self.line == 0 && self.column == 0 && self.id == 0
    }
}

impl Default for Position {
    fn default() -> Self {
        Position::new(0, 0, 0)
    }
}

impl From<Position> for legacy::Position {
    fn from(position: Position) -> legacy::Position {
        legacy::Position::new(
            position.line,
            position.column,
            position.id
        )
    }
}

pub enum PermAmountError {
    InvalidAdd(PermAmount, PermAmount),
    InvalidSub(PermAmount, PermAmount)
}

impl From<PermAmountError> for legacy::PermAmountError {
    fn from(perm_amount_error: PermAmountError) -> legacy::PermAmountError {
        match perm_amount_error {
            PermAmountError::InvalidAdd(perm_amount_1, perm_amount_2) => legacy::PermAmountError::InvalidAdd(legacy::PermAmount::from(perm_amount_1), legacy::PermAmount::from(perm_amount_2)),
            PermAmountError::InvalidSub(perm_amount_1, perm_amount_2) => legacy::PermAmountError::InvalidSub(legacy::PermAmount::from(perm_amount_1), legacy::PermAmount::from(perm_amount_2)),
        }
    }
}

/// The permission amount.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PermAmount {
    Read,
    Write,
    /// The permission remaining after ``Read`` was subtracted from ``Write``.
    Remaining,
}

impl fmt::Display for PermAmount {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PermAmount::Read => write!(f, "read"),
            PermAmount::Write => write!(f, "write"),
            PermAmount::Remaining => write!(f, "write-read"),
        }
    }
}

impl PartialOrd for PermAmount {
    fn partial_cmp(&self, other: &PermAmount) -> Option<Ordering> {
        match (self, other) {
            (PermAmount::Read, PermAmount::Write) => Some(Ordering::Less),
            (PermAmount::Read, PermAmount::Read) | (PermAmount::Write, PermAmount::Write) => {
                Some(Ordering::Equal)
            }
            (PermAmount::Write, PermAmount::Read) => Some(Ordering::Greater),
            _ => None,
        }
    }
}

impl Ord for PermAmount {
    fn cmp(&self, other: &PermAmount) -> Ordering {
        self.partial_cmp(other).expect(&format!(
            "Undefined comparison between {:?} and {:?}",
            self, other
        ))
    }
}

impl From<PermAmount> for legacy::PermAmount {
    fn from(perm_amount: PermAmount) -> legacy::PermAmount {
        match perm_amount {
            PermAmount::Read => legacy::PermAmount::Read,
            PermAmount::Write => legacy::PermAmount::Write,
            PermAmount::Remaining => legacy::PermAmount::Remaining,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Type {
    Int,
    Bool,
    Seq(Box<Type>),
    //Ref, // At the moment we don't need this
    /// TypedRef: the first parameter is the name of the predicate that encodes the type
    TypedRef(String, Vec<Type>),
    Domain(String, Vec<Type>),
    TypedVar(String),
    Snapshot(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeId {
    Int,
    Bool,
    Ref,
    Seq,
    Domain,
    Snapshot,
}

impl fmt::Display for Type {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Type::Int => write!(f, "Int"),
            Type::Bool => write!(f, "Bool"),
            //Type::Ref => write!(f, "Ref"),
            Type::TypedRef(ref name, _) => write!(f, "Ref({})", name),
            Type::Domain(ref name, _) => write!(f, "Domain({})", name),
            Type::Snapshot(ref name) => write!(f, "Snapshot({})", name),
            Type::Seq(ref elem_ty) => write!(f, "Seq[{}]", elem_ty),
            Type::TypedVar(ref name) => write!(f, "TypedVar({})", name),
        }
    }
}

impl PartialEq for Type {
    fn eq(&self, other: &Self) -> bool {
        discriminant(self) == discriminant(other)
    }
}

impl Eq for Type {}

impl Hash for Type {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
    }
}

impl From<Type> for legacy::Type {
    fn from(typ: Type) -> legacy::Type {
        match typ {
            Type::Int => legacy::Type::Int,
            Type::Bool => legacy::Type::Bool,
            Type::Seq(elem_ty) => legacy::Type::Seq(Box::new(legacy::Type::from(*elem_ty.clone()))),
            Type::TypedRef(label, _) => legacy::Type::TypedRef(label.clone()),
            Type::Domain(label, _) => legacy::Type::Domain(label.clone()),
            // FIXME: needs update for type substitution
            Type::TypedVar(label) => legacy::Type::TypedRef(label.clone()),
            Type::Snapshot(label) => legacy::Type::Snapshot(label.clone()),
        }
    }
}

impl From<TypeId> for legacy::TypeId {
    fn from(type_id: TypeId) -> legacy::TypeId {
        match type_id {
            TypeId::Int => legacy::TypeId::Int,
            TypeId::Bool => legacy::TypeId::Bool,
            TypeId::Ref => legacy::TypeId::Ref,
            TypeId::Seq => legacy::TypeId::Seq,
            TypeId::Domain => legacy::TypeId::Domain,
            TypeId::Snapshot => legacy::TypeId::Snapshot,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LocalVar {
    pub name: String,
    pub typ: Type,
}

impl fmt::Display for LocalVar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl fmt::Debug for LocalVar {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.typ)
    }
}

impl From<LocalVar> for legacy::LocalVar {
    fn from(type_id: LocalVar) -> legacy::LocalVar {
        legacy::LocalVar {
            name: type_id.name.clone(),
            typ: legacy::Type::from(type_id.typ.clone()),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    pub typ: Type,
}

impl fmt::Display for Field {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl fmt::Debug for Field {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {}", self.name, self.typ)
    }
}

impl From<Field> for legacy::Field {
    fn from(field: Field) -> legacy::Field {
        legacy::Field {
            name: field.name.clone(),
            typ: legacy::Type::from(field.typ.clone()),
        }
    }
}