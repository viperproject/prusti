use std::collections::HashMap;

use log::{info, warn};
use prusti_common::vir;

pub use self::purifier::{AssertPurifier, ExprPurifier};

use super::{errors::EncodingResult, snapshot_encoder::{SNAPSHOT_VARIANT,Snapshot}};

mod fixer;
mod purifier;
pub mod optimizer;

pub const NAT_DOMAIN_NAME: &str = "$Nat$";
pub const AXIOMATIZED_FUNCTION_DOMAIN_NAME: &str = "$MirrorFunctions$";
pub const PRIMITIVE_VALID_DOMAIN_NAME: &str = "PrimitiveValidDomain";
pub const MIRROR_FUNCTION_PREFIX: &str = "mirrorfn$";
const MIRROR_FUNCTION_CALLER_PREFIX: &str = "caller_for$$";

pub fn mirror_function_caller_call(mirror_fn: vir::DomainFunc, args: Vec<vir::Expr>) -> vir::Expr {
    let caller_func_name = caller_function_name(&mirror_fn.name);
    vir::Expr::FuncApp(
        caller_func_name,
        args,
        mirror_fn.formal_args,
        mirror_fn.return_type,
        Default::default(),
    )
}

pub fn encode_variant_func(domain_name: String) -> vir::DomainFunc
{
    let snap_type = vir::Type::Domain(domain_name.to_string());
    let arg = vir::LocalVar::new("self", snap_type);
    vir::DomainFunc {
        name: SNAPSHOT_VARIANT.to_string(),
        formal_args: vec![arg],
        return_type: vir::Type::Int,
        unique: false,
        domain_name: domain_name.to_string(),
    }
}

pub fn caller_function_name(df_name: &str) -> String {
    format!("{}{}", MIRROR_FUNCTION_CALLER_PREFIX, df_name)
}

pub fn encode_field_domain_func(
    field_type: vir::Type,
    field_name: String,
    domain_name: String,
    variant_name: Option<String>,
) -> vir::DomainFunc {
    let mut field_domain_name = domain_name.clone();
    if let Some(s) = variant_name {
        field_domain_name += &s;
    }
    let return_type: vir::Type = match field_type {
        vir::Type::TypedRef(name) => vir::Type::Domain(name),
        t => t,
    };

    vir::DomainFunc {
        name: format!("{}$field${}", field_domain_name, field_name), //TODO get the right name
        formal_args: vec![vir::LocalVar {
            name: "self".to_string(),
            typ: vir::Type::Domain(domain_name.to_string()),
        }],
        return_type,
        unique: false,
        domain_name: domain_name.to_string(),
    }
}

pub fn encode_unfold_witness(domain_name: String) -> vir::DomainFunc {
    let self_type = vir::Type::Domain(domain_name.clone());
    let self_arg = vir::LocalVar {
        name: "self".to_string(),
        typ: self_type,
    };

    let nat_type = vir::Type::Domain(NAT_DOMAIN_NAME.to_owned());
    let nat_arg = vir::LocalVar {
        name: "count".to_string(),
        typ: nat_type,
    };

    vir::DomainFunc {
        name: format!("{}$UnfoldWitness", domain_name),
        formal_args: vec![self_arg, nat_arg],
        return_type: vir::Type::Bool,
        unique: false,
        domain_name,
    }
}

/// Returns the T$valid function for the given type
pub fn valid_func_for_type(typ: &vir::Type) -> vir::DomainFunc {
    let domain_name: String = match typ {
        vir::Type::Domain(name) => name.clone(),
        vir::Type::Bool | vir::Type::Int => PRIMITIVE_VALID_DOMAIN_NAME.to_string(),
        vir::Type::TypedRef(_) => unreachable!(),
    };

    let arg_typ: vir::Type = match typ {
        vir::Type::Domain(name) => vir::Type::Domain(domain_name.clone()),
        vir::Type::Bool => vir::Type::Bool,
        vir::Type::Int => vir::Type::Int,
        vir::Type::TypedRef(_) => unreachable!(),
    };

    let self_arg = vir::LocalVar {
        name: "self".to_string(),
        typ: arg_typ,
    };
    let df = vir::DomainFunc {
        name: format!("{}$valid", domain_name),
        formal_args: vec![self_arg],
        return_type: vir::Type::Bool,
        unique: false,
        domain_name,
    };

    df
}

/// Returns the LocalVar that is the Nat argument used in axiomatized functions
pub fn encode_nat_argument() -> vir::LocalVar {
    vir::LocalVar {
        name: "count".to_string(),
        typ: vir::Type::Domain(NAT_DOMAIN_NAME.to_owned()),
    }
}

/// Returns the arguments for the axiomatized version of a function but does not yet include the Nat argument
pub fn encode_mirror_function_args_without_nat(
    formal_args: &[vir::LocalVar],
    snapshots: &HashMap<String, Box<Snapshot>>,
) -> Result<Vec<vir::LocalVar>, String> {
    formal_args
        .iter()
        .map(|e| {
            let old_type = e.typ.clone();
            let new_type = translate_type(old_type, &snapshots)?;

            Ok(vir::LocalVar {
                name: e.name.clone(),
                typ: new_type,
            })
        })
        .collect()
}

// TODO: CHange the return type to return a proper error instead of the String.
pub fn encode_mirror_function(
    name: &str,
    formal_args: &[vir::LocalVar],
    return_type: &vir::Type,
    snapshots: &HashMap<String, Box<Snapshot>>,
) -> Result<vir::DomainFunc, String> {
    let formal_args_without_nat: Vec<vir::LocalVar> =
        encode_mirror_function_args_without_nat(formal_args, snapshots)?;

    let mut formal_args = formal_args_without_nat.clone();
    formal_args.push(encode_nat_argument());

    let df = vir::DomainFunc {
        name: format!("{}{}", MIRROR_FUNCTION_PREFIX, name),
        formal_args: formal_args.clone(),
        return_type: translate_type(return_type.clone(), &snapshots)?,
        unique: false,
        domain_name: AXIOMATIZED_FUNCTION_DOMAIN_NAME.to_owned(),
    };

    Ok(df)
}

fn unbox(name: String) -> String {
    let start = "m_Box$_beg_$";
    let end = "$_sep_$m_Global$_beg_$_end_$_end_";
    if !name.ends_with(end) {
        return name;
    }

    if !name.starts_with(start) {
        return name;
    }

    let remaining = name.len() - start.len() - end.len();

    return name.chars().skip(start.len()).take(remaining).collect();
}

pub fn translate_type(t: vir::Type, snapshots: &HashMap<String, Box<Snapshot>>) -> Result<vir::Type, String> {
    match t {
        vir::Type::TypedRef(name) => match name.as_str() {
            "i32" | "usize" | "u32" => Ok(vir::Type::Int),
            "bool" => Ok(vir::Type::Bool),
            _ => {
                let name = unbox(name);
                let domain_name = snapshots
                    .get(&name)
                    .and_then(|snap| snap.domain())
                    .map(|domain| domain.name)
                    .ok_or(format!(
                        "No matching domain for '{}' in '{:?}'",
                        name,
                        snapshots.keys(),
                    ))?;

                Ok(vir::Type::Domain(domain_name))
            }
        },
        o @ _ => Ok(o),
    }
}

/// Fix assertion by purifying heap dependent function calls that get snapshot
/// argument.
pub fn fix_assertion(
    assertion: vir::Expr,
    snapshots: &HashMap<String, Box<Snapshot>>,
) -> vir::Expr {
    vir::ExprFolder::fold(&mut fixer::Fixer { snapshots }, assertion)
}

pub fn get_succ_func() -> vir::DomainFunc {
    let succ = vir::DomainFunc {
        name: "succ".to_owned(),
        formal_args: vec![vir::LocalVar {
            name: "val".to_owned(),
            typ: vir::Type::Domain(NAT_DOMAIN_NAME.to_owned()),
        }],
        return_type: vir::Type::Domain(NAT_DOMAIN_NAME.to_owned()),
        unique: false,
        domain_name: NAT_DOMAIN_NAME.to_owned(),
    };

    succ
}

pub fn get_zero_func() -> vir::DomainFunc {
    let zero = vir::DomainFunc {
        name: "zero".to_owned(),
        formal_args: Vec::new(),
        return_type: vir::Type::Domain(NAT_DOMAIN_NAME.to_owned()),
        unique: false,
        domain_name: NAT_DOMAIN_NAME.to_owned(),
    };

    zero
}

pub fn succ_of(e: vir::Expr) -> vir::Expr {
    let succ_func = get_succ_func();
    vir::Expr::domain_func_app(succ_func, vec![e])
}

fn plus_n_nat(e: vir::Expr, n: u32) -> vir::Expr {
    let mut res = e;
    for _ in 0..n {
        res = succ_of(res);
    }

    res
}

pub fn zero_nat() -> vir::Expr {
    let zero_func = get_zero_func();
    vir::Expr::domain_func_app(zero_func, vec![])
}

pub fn one_nat() -> vir::Expr {
    succ_of(zero_nat())
}

pub fn two_nat() -> vir::Expr {
    succ_of(one_nat())
}

pub fn n_nat(n: u32) -> vir::Expr {
    plus_n_nat(zero_nat(), n)
}

pub fn result_is_valid(typ: &vir::Type) -> vir::Expr {
    let self_var = vir::LocalVar {
        name: "__result".to_owned(),
        typ: typ.clone(),
    };
    let valid_func = valid_func_for_type(&typ);
    let self_arg = vir::Expr::local(self_var.clone());
    vir::Expr::domain_func_app(valid_func, vec![self_arg])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_unbox() {
        let res = unbox(
            "m_Box$_beg_$m_len_lookup$$List$_beg_$_end_$_sep_$m_Global$_beg_$_end_$_end_"
                .to_string(),
        );
        assert_eq!(res, "m_len_lookup$$List$_beg_$_end_".to_string());
        assert_eq!(unbox("u32".to_string()), "u32".to_string());
    }

    #[test]
    fn test_nat_basic() {
        assert_eq!(n_nat(0), zero_nat());
        assert_eq!(n_nat(1), one_nat());
        assert_eq!(n_nat(2), two_nat());
        assert_eq!(n_nat(6), succ_of(succ_of(succ_of(succ_of(two_nat())))));

        assert_ne!(zero_nat(), two_nat());
        assert_ne!(zero_nat(), one_nat());
        assert_ne!(n_nat(12), n_nat(13));
    }
    #[test]
    fn test_nat_plus() {
        assert_eq!(plus_n_nat(zero_nat(), 1), n_nat(1));

        assert_eq!(plus_n_nat(one_nat(), 1), n_nat(2));

        assert_eq!(plus_n_nat(two_nat(), 1), n_nat(3));
        assert_eq!(plus_n_nat(two_nat(), 4), n_nat(6));
        assert_eq!(plus_n_nat(two_nat(), 0), n_nat(2));
        assert_eq!(plus_n_nat(n_nat(2), 5), n_nat(7));
    }
}
