// © 2019, ETH Zurich
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

use crate::vir::ast::*;

impl From<LocalVar> for Expr {
    fn from(local_var: LocalVar) -> Self {
        Expr::Local(local_var, Position::default())
    }
}

impl<'a> From<&'a LocalVar> for Expr {
    fn from(local_var: &'a LocalVar) -> Self {
        Expr::Local(local_var.clone(), Position::default())
    }
}

impl From<Const> for Expr {
    fn from(cons: Const) -> Self {
        Expr::Const(cons, Position::default())
    }
}

impl From<bool> for Const {
    fn from(val: bool) -> Self {
        Const::Bool(val)
    }
}

impl From<bool> for Expr {
    fn from(val: bool) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<usize> for Const {
    fn from(val: usize) -> Self {
        Const::BigInt(val.to_string())
    }
}

impl From<usize> for Expr {
    fn from(val: usize) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<isize> for Const {
    fn from(val: isize) -> Self {
        Const::BigInt(val.to_string())
    }
}

impl From<isize> for Expr {
    fn from(val: isize) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<i8> for Const {
    fn from(val: i8) -> Self {
        Const::Int(val as i64)
    }
}

impl From<i8> for Expr {
    fn from(val: i8) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<i16> for Const {
    fn from(val: i16) -> Self {
        Const::Int(val as i64)
    }
}

impl From<i16> for Expr {
    fn from(val: i16) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<i32> for Const {
    fn from(val: i32) -> Self {
        Const::Int(val as i64)
    }
}

impl From<i32> for Expr {
    fn from(val: i32) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<i64> for Const {
    fn from(val: i64) -> Self {
        Const::Int(val as i64)
    }
}

impl From<i64> for Expr {
    fn from(val: i64) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<i128> for Expr {
    fn from(val: i128) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<i128> for Const {
    fn from(val: i128) -> Self {
        Const::BigInt(val.to_string())
    }
}

impl From<u8> for Expr {
    fn from(val: u8) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<u8> for Const {
    fn from(val: u8) -> Self {
        Const::Int(val as i64)
    }
}

impl From<u16> for Expr {
    fn from(val: u16) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<u16> for Const {
    fn from(val: u16) -> Self {
        Const::Int(val as i64)
    }
}

impl From<u32> for Expr {
    fn from(val: u32) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<u32> for Const {
    fn from(val: u32) -> Self {
        Const::Int(val as i64)
    }
}

impl From<u64> for Expr {
    fn from(val: u64) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<u64> for Const {
    fn from(val: u64) -> Self {
        Const::BigInt(val.to_string())
    }
}

impl From<u128> for Expr {
    fn from(val: u128) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<char> for Const {
    fn from(val: char) -> Self {
        Const::BigInt((val as u128).to_string())
    }
}

impl From<char> for Expr {
    fn from(val: char) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl From<u128> for Const {
    fn from(val: u128) -> Self {
        Const::BigInt(val.to_string())
    }
}

impl<'a> From<&'a str> for Expr {
    fn from(val: &'a str) -> Self {
        Expr::Const(val.into(), Position::default())
    }
}

impl<'a> From<&'a str> for Const {
    fn from(val: &'a str) -> Self {
        Const::BigInt(val.to_string())
    }
}
