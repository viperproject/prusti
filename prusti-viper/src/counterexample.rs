use prusti_interface::data::ProcedureDefId;
use std::collections::HashMap;
use viper::silicon_counterexample::*;
use crate::encoder::Encoder;
use viper::VerificationError;



pub struct Counterexample {
    pub entries: HashMap<String, Entry>
}

pub fn backtranslate(
    def_id: ProcedureDefId, 
) -> Counterexample {
    Counterexample{
        entries: HashMap::new()
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