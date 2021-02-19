use prusti_interface::data::ProcedureDefId;
use prusti_common::vir::CfgMethod;
use std::collections::HashMap;
use viper::silicon_counterexample::*;
use crate::encoder::Encoder;
use viper::VerificationError;



pub enum Counterexample {
    Success{
        entries: HashMap<String, Entry>
    },
    Failure(String),
}

pub fn backtranslate(
    def_id: ProcedureDefId,
    cfg_method: CfgMethod,
    silicon_counterexample: Option<SiliconCounterexample>,
) -> Counterexample {
    match silicon_counterexample{
        None => Counterexample::Failure(String::from("Silicon-counterexample not available")),
        Some(sil_ce) => {
            let silicon_entries = sil_ce.model.entries;

            for (rust_name, mir_name) in cfg_method.var_debug_info{
                let entry = silicon_entries.get(&mir_name);
                println!("{} <- {:?}", rust_name, entry);
            }
            Counterexample::Failure(String::from("Actually success"))
        }
    }
}


pub enum Entry{
    IntEntry{value: i64},
    BoolEntry{value: bool},
    CharEntry{value: char},
    RefEntry{el: Box<Entry>},
    AlgebraicDT{
        name: String,
        fields: HashMap<String, Box<Entry>>
    },
    UnknownEntry
}