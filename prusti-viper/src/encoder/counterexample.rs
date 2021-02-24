use prusti_interface::data::ProcedureDefId;
use prusti_common::vir::CfgMethod;
use std::collections::HashMap;
use viper::silicon_counterexample::*;
use crate::encoder::Encoder;
use viper::VerificationError;
use crate::encoder::places::{Local, LocalVariableManager, Place};
use std::convert::TryFrom;

use rustc_middle::mir;
use rustc_middle::ty::{self, Ty, AdtKind, AdtDef, TyCtxt};



pub enum Counterexample {
    Success{
        result: Entry,
        args: HashMap<String, Entry>,
        entries: HashMap<String, Entry>
    },
    Failure(String),
}
#[derive(Debug)]
pub enum Entry{
    IntEntry{value: i64},
    BoolEntry{value: bool},
    CharEntry{value: char},
    RefEntry{el: Box<Entry>},
    Struct{
        name: String,
        fields: HashMap<String, Entry>
    },
    Enum{
        super_name: String,
        name: String,
        named_fields: bool,
        fields: HashMap<String, Entry>
        //note: if fields are not named, their order is important!
    },
    UnknownEntry
}

pub fn backtranslate<'tcx>(
    silicon_counterexample: Option<SiliconCounterexample>,
    mir: &mir::Body<'tcx>,
    is_pure: bool,
    tcx: &TyCtxt<'tcx> 
) -> Counterexample {
    if let Some(silicon_ce) = silicon_counterexample {
        //get all the needed information from the mir
        let var_debug_info = mir.var_debug_info.clone();
        let local_variable_manager = LocalVariableManager::new(&mir.local_decls);
        let arg_count = mir.arg_count;
        let last_label = silicon_ce.get_last_label();
        println!("last label: {:?}", last_label);
        //args:
 

        let mut entries = HashMap::new();
        let mut args = HashMap::new();
        for vdi in var_debug_info{
            let rust_name = vdi.name.to_ident_string();
            println!("evaluating variable {}", rust_name);
            let local: mir::Local = if let mir::VarDebugInfoContents::Place(place) = vdi.value {
                if let Some(local) = place.as_local() {
                    local
                } else {
                    unimplemented!();
                }
            } else {
                unimplemented!();
            };
            let index = local.index();
            let var_local = Local::from(local);
            let typ = local_variable_manager.get_type(var_local);
            let vir_name = local_variable_manager.get_name(var_local);
            println!("vir name {}", vir_name);
            let opt_entry = silicon_ce.get_entry_at_label(&vir_name, last_label);
            let entry = backtranslate_entry(typ, opt_entry, tcx);
            entries.insert(rust_name.clone(), entry);

            if index > 0 && index <= arg_count {
                let opt_entry = silicon_ce.get_entry_at_label(&vir_name, Some(&String::from("l0")));
                let arg_entry = backtranslate_entry(typ, opt_entry, tcx);
                args.insert(rust_name, arg_entry);
            }


        }

        //for the return type:
        let return_local = Local::from(mir::Local::from_usize(0));
        //make sure
        let result = if local_variable_manager.is_return(return_local) {
            let rust_name = String::from("result");
            let vir_name = local_variable_manager.get_name(return_local);
            let typ = local_variable_manager.get_type(return_local);
            let opt_entry = silicon_ce.get_entry_at_label(&vir_name, last_label);
            backtranslate_entry(typ, opt_entry, tcx)
        } else {
            Entry::UnknownEntry
        };



        println!("arguments: {:?}", args);
        println!("final entries: {:?}", entries);
        println!("result <- {:?}", result);
    }
    Counterexample::Failure(String::from("there"))

    /*
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
    }*/
}

fn backtranslate_entry<'tcx>(typ: Ty<'tcx>, sil_entry: Option<&ModelEntry>, tcx: &TyCtxt<'tcx>) -> Entry {
    match sil_entry {
        None => Entry::UnknownEntry,
        Some(entry) => {
            println!("Translating entry: {:?}", entry);
            match typ.kind(){
                ty::TyKind::Bool => {
                    match entry{
                        ModelEntry::LitBoolEntry(value) => Entry::BoolEntry { value: *value },
                        ModelEntry::RefEntry(name, map) => {
                            let entry = map.get(&String::from("val_bool"));
                            match entry{
                                Some(ModelEntry::LitBoolEntry(value)) => Entry::BoolEntry { value: *value},
                                _ => Entry::UnknownEntry
                            }    
                        },
                        _ => Entry::UnknownEntry
                    }
                },
                ty::TyKind::Int(_) | ty::TyKind::Uint(_) => {
                    match entry{
                        ModelEntry::LitIntEntry(value) => Entry::IntEntry{ value: *value },
                        ModelEntry::RefEntry(name, map) => {
                            let entry = map.get(&String::from("val_int"));
                            match entry{
                                Some(ModelEntry::LitIntEntry(value)) => Entry::IntEntry{ value: *value },
                                _ => Entry::UnknownEntry
                            }
                        }
                        _ => Entry::UnknownEntry
                    }
                },
                ty::TyKind::Char => {
                    match entry{
                        ModelEntry::LitIntEntry(value) => {
                            let val_t = u8::try_from(*value).ok();
                            match val_t {
                                Some(v) => Entry::CharEntry{ value: v as char},
                                None => Entry::UnknownEntry
                            }
                        },
                        ModelEntry::RefEntry(name, map) => {
                            let entry = map.get(&String::from("val_int"));
                            match entry {
                                Some(ModelEntry::LitIntEntry(value)) => {
                                    let val_t = u8::try_from(*value).ok();
                                    match val_t {
                                        Some(v) => Entry::CharEntry{ value: v as char},
                                        None => Entry::UnknownEntry
                                    }
                                },
                                _ => Entry::UnknownEntry
                            }
                        },
                        _ => Entry::UnknownEntry
                    }
                },
                ty::TyKind::Ref(_, typ, _) => {
                    match entry{
                        ModelEntry::RefEntry(name, map) => {
                            let entry = map.get(&String::from("val_ref"));
                            match entry {
                                Some(e) => {
                                    let rec_entry = backtranslate_entry(typ, Some(e), tcx);
                                    Entry::RefEntry{ el: Box::new(rec_entry)} 
                                },
                                _ => Entry::UnknownEntry
                            }
                        },
                        _ => Entry::UnknownEntry
                    }
                }
                ty::TyKind::Adt(adt_def, subst) => {
                    match adt_def.adt_kind() {
                        AdtKind::Struct => {
                            let struct_name = adt_def.variants.iter().next().unwrap().ident.name.to_ident_string();
                            let fields = adt_def.all_fields();
                            let mut ce_fields = HashMap::new();

                            for f in fields {
                                let name = f.ident.name.to_ident_string();
                                let mut sil_name = String::from("f$");
                                sil_name.push_str(&name);
                                let field_typ = f.ty(*tcx, subst);
                                
                                match entry {
                                    ModelEntry::RefEntry(_, map) => {
                                        let entry = map.get(&sil_name);
                                        if let Some(e) = entry {
                                            let rec_entry = backtranslate_entry(field_typ, Some(e), tcx);
                                            ce_fields.insert(name, rec_entry);
                                        }
                                    },
                                    _ => ()
                                }
                            }
                            Entry::Struct{
                                name: struct_name,
                                fields: ce_fields,
                            }
                        },
                        AdtKind::Enum => {
                            let variants = &adt_def.variants.iter();
                            Entry::UnknownEntry
                        },
                        _ => unimplemented!()
                    }
                }

                _ => Entry::UnknownEntry
            }

            /*
            ty::TyKind::Ref(_, _, _)
            ty::TyKind::Adt(_, _)
            ty::TyKind::Tuple(_)
            ty::TyKind::Never
            ty::TyKind::Param(_),
            */
        }
    }
}

