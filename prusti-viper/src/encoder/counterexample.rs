use prusti_interface::data::ProcedureDefId;
use prusti_common::vir::CfgMethod;
use std::collections::HashMap;
use viper::silicon_counterexample::*;
use crate::encoder::Encoder;
use viper::VerificationError;
use crate::encoder::places::{Local, LocalVariableManager, Place};
use std::convert::TryFrom;
use std::fmt;

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

impl Counterexample{
    pub fn emit(&self) {
        match self{
            Counterexample::Success{result, args, entries} => {
                println!("\nCounterexample:");
                println!("initial args:");
                for (name, entry) in args {
                    println!("{} <- {}", name, entry);
                }
                println!("\nlocal values when failing:");
                for (name, entry) in entries {
                    println!("{} <- {}", name, entry);
                }
                println!("\nresult <- {}", result)
            },
            _ => ()
        }
    }
}



#[derive(Debug)]
pub enum Entry{
    IntEntry{value: i64},
    BoolEntry{value: bool},
    CharEntry{value: char},
    RefEntry{el: Box<Entry>},
    Struct{
        name: String,
        field_names: Vec<String>,
        field_entries: Vec<Entry>
    },
    Enum{
        super_name: String,
        name: String,
        named_fields: bool,
        field_names: Vec<String>,
        field_entries: Vec<Entry>
        //note: if fields are not named, their order is important!
    },
    Tuple{
        fields: Vec<Entry>
    },
    Unit,
    UnknownEntry
}

impl fmt::Display for Entry{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self{
            Entry::IntEntry { value } => write!(f, "{}", value),
            Entry::BoolEntry { value } => write!(f, "{}", value),
            Entry::CharEntry { value } => write!(f, "'{}' ({:x})", value, *value as i32),
            Entry::RefEntry { el } => write!(f, "ref( {})", *el),
            Entry::Enum { super_name, name, named_fields, field_names, field_entries } => {
                write!(f, "{}::{}", super_name, name);
                let length = field_entries.len();
                if length > 0{
                    if *named_fields {
                        write!(f, "{{");
                        for i in 0..length{
                            write!(f, "\n\t{} <- {}", field_names[i], field_entries[i]);
                        }
                        write!(f, "\n}}")
                    } else {
                        write!(f, "(");
                        let len = length - 1;
                        for (i, entry) in (*field_entries).iter().enumerate(){
                            if i < len {
                                write!(f, "{}, ", entry);
                            } else {
                                write!(f, "{}", entry);
                        }
                }
                write!(f, ")\n")
                    }
                } else {
                    write!(f, "")
                }
            }
            Entry::Struct { name, field_names, field_entries} => {
                write!(f, "{} {{", name);
                let len = field_names.len();
                for i in 0..len{
                    write!(f, "\n\t{} <- {}", field_names[i], field_entries[i]);
                }
                write!(f, "\n}}\n")
            },
            Entry::Tuple { fields } => {
                write!(f, "(");
                let len = (*fields).len() - 1;
                for (i, entry) in (*fields).iter().enumerate(){
                    if i < len {
                        write!(f, "{}, ", entry);
                    } else {
                        write!(f, "{}", entry);
                    }
                }
                write!(f, ")\n")
            },
            Entry::Unit => write!(f, "()"),
            _ => write!(f, "?")
        }
    }
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
        let last_label = silicon_ce.label_order.last();
        
        //to be processed:
        let mut args_to_process: Vec<(String, String, Ty)> = vec![];
        let mut entries_to_process: Vec<(String, String, Ty)> = vec![];

        for vdi in var_debug_info{
            let rust_name = vdi.name.to_ident_string();
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
            entries_to_process.push((rust_name.clone(), vir_name.clone(), typ));
/*
            let opt_entry = silicon_ce.get_entry_at_label(&vir_name, last_label);
            let entry = backtranslate_entry(typ, opt_entry, tcx);
            entries.insert(rust_name.clone(), entry);
*/
            if index > 0 && index <= arg_count {
                args_to_process.push((rust_name, vir_name, typ))
                /*let opt_entry = silicon_ce.get_entry_at_label(&vir_name, Some(&String::from("l0")));
                let arg_entry = backtranslate_entry(typ, opt_entry, tcx);
                args.insert(rust_name, arg_entry);*/
            }
        }

        //for the return type:
        let return_local = Local::from(mir::Local::from_usize(0));
        //make sure
        let result_to_process = if local_variable_manager.is_return(return_local){
            let rust_name = String::from("result");
            let vir_name = if !is_pure {
                local_variable_manager.get_name(return_local)
            } else {
                String::from("Result()")
            };
            let typ = local_variable_manager.get_type(return_local);
            Some((rust_name, vir_name, typ))
        } else {
            None
        };
        

        //now map those needed:
        let mut entries = HashMap::new();
        let mut args = HashMap::new();
        let result = if !is_pure {
            for (rust_name, vir_name, typ) in args_to_process {
                let opt_entry = silicon_ce.get_entry_at_label(&vir_name, Some(&String::from("l8")));
                let entry = backtranslate_entry(typ, opt_entry, tcx);
                args.insert(rust_name, entry);
            }
            for (rust_name, vir_name, typ) in entries_to_process {
                let opt_entry = silicon_ce.get_entry_at_label(&vir_name, last_label);
                let entry = backtranslate_entry(typ, opt_entry, tcx);
                entries.insert(rust_name, entry);
            }
            match result_to_process {
                None => Entry::Unit,
                Some((rust_name, vir_name, typ)) => {
                    let opt_entry = silicon_ce.get_entry_at_label(&vir_name, last_label);
                    backtranslate_entry(typ, opt_entry, tcx)
                }
            }
        } else {
            for (rust_name, vir_name, typ) in args_to_process {
                let opt_entry = silicon_ce.get_entry_at_label(&vir_name, None);
                let entry = backtranslate_entry(typ, opt_entry, tcx);
                args.insert(rust_name, entry);
            }
            match result_to_process {
                None => Entry::Unit,
                Some((rust_name, vir_name, typ)) => {
                    let opt_entry = silicon_ce.get_entry_at_label(&vir_name, None);
                    backtranslate_entry(typ, opt_entry, tcx)
                }
            }
        };
        
        println!("arguments: {:?}", args);
        println!("final entries: {:?}", entries);
        println!("result <- {:?}", result);
        Counterexample::Success{ result, args, entries}
    } else {
        Counterexample::Failure(String::from("there"))
    }
}



fn backtranslate_entry<'tcx>(typ: Ty<'tcx>, sil_entry: Option<&ModelEntry>, tcx: &TyCtxt<'tcx>) -> Entry {
    match sil_entry {
        None => Entry::UnknownEntry,
        Some(entry) => {
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
                },
                ty::TyKind::Tuple(subst) => {
                    match entry{
                        ModelEntry::RefEntry(name, map) => {
                            let mut fields = vec![];
                            let len = subst.types().count();
                            for i in 0..len{
                                let typ = subst.type_at(i);
                                let field_id = format!("tuple_{}", i).to_string();
                                let field_entry = map.get(&field_id);
                                let rec_entry = backtranslate_entry(typ, field_entry, tcx);
                                fields.push(rec_entry);
                            }
                            Entry::Tuple{fields}
                        },
                        _ => Entry::UnknownEntry
                    }
                },
                ty::TyKind::Adt(adt_def, subst) => {
                    match adt_def.adt_kind() {
                        AdtKind::Struct => {
                            let mut field_names = vec![];
                            let mut field_entries = vec![];
                            let struct_name = adt_def.variants.iter().next().unwrap().ident.name.to_ident_string();
                            match entry{
                                ModelEntry::RefEntry(_, map) => {
                                    let fields = adt_def.all_fields();

                                    for f in fields {
                                        let name = f.ident.name.to_ident_string();
                                        let sil_name = format!{"f${}", name}.to_string(); 
                                        let field_typ = f.ty(*tcx, subst);
                                        let entry = map.get(&sil_name);
                                        let rec_entry = backtranslate_entry(field_typ, entry, tcx);
                                        field_names.push(name);
                                        field_entries.push(rec_entry);
                                    }
                                },
                                _ => ()
                            }
                            Entry::Struct{
                                name: struct_name,
                                field_names,
                                field_entries
                            }
                        },
                        AdtKind::Enum => {
                            let variants = &adt_def.variants.iter();
                            let super_name = format!("{:?}", adt_def);
                            let mut variant_name = String::from("?");
                            let mut field_names = vec![];
                            let mut field_entries = vec![];
                            match entry{
                                ModelEntry::RefEntry(_, m) => {
                                    let map = m.clone();
                                    let disc_string = String::from("discriminant");
                                    let mut variant = None;

                                    {
                                    let disc_entry = map.get(&disc_string);

                                    //need to find a discriminant to do something
                                    match disc_entry {
                                        Some(&ModelEntry::LitIntEntry(x)) => {
                                            let discriminant = x as u32;
                                            //is this a risk? I assume discriminant will not be too large
                                            let variants = &adt_def.variants;
                                            for def in variants{
                                                let discr = match def.discr {
                                                    ty::VariantDiscr::Relative(d) => d,
                                                    _ => unimplemented!()  
                                                };
                                                if discr == discriminant {
                                                    variant = Some(def);
                                                    variant_name = def.ident.name.to_ident_string();
                                                }
                                            }
                                        },
                                        _ => ()
                                    }}

                                    match variant {
                                        Some(var_def) => {
                                            let sil_name = format!("enum_{}", variant_name).to_string();
                                            let opt_enum_entry = m.get(&sil_name);
                                            println!("found entry: {:?}", opt_enum_entry); 
                                            //at this point it should be a subroutine same for structs and enum:
                                            match opt_enum_entry {
                                                //do the extraction
                                                Some(ModelEntry::RefEntry(refname, map2)) => {
                                                    for f in &var_def.fields{
                                                        let field_name = f.ident.name.to_ident_string();
                                                        let typ = f.ty(*tcx, subst);
                                                        println!("field: {} : {:?}", field_name, typ);

                                                        //extract recursively:
                                                        let sil_name = format!("f${}", field_name).to_string();
                                                        let rec_entry = map2.get(&sil_name);
                                                        println!("found entry  {:?} for {}", rec_entry, sil_name);
                                                        let field_entry = match rec_entry {
                                                            Some(ModelEntry::RecursiveRefEntry(name)) => {
                                                                assert!(refname == name);
                                                                backtranslate_entry(typ, Some(entry), tcx)
                                                            },
                                                            _ => backtranslate_entry(typ, rec_entry, tcx),
                                                        };
                                                        field_names.push(field_name);
                                                        field_entries.push(field_entry);
                                                    }
                                                },
                                                _ => (),
                                            }
                                        },
                                        None => ()
                                    }
                                },
                                _ => ()
                            }
                            let named_fields = field_names.len() > 0 && field_names[0].parse::<usize>().is_ok();

                            Entry::Enum{
                                super_name,
                                name: variant_name,
                                named_fields,
                                field_names,
                                field_entries
                            }
                        },
                        _ => unimplemented!()
                    }
                },

                _ => Entry::UnknownEntry
            }

            /*
            ty::TyKind::Adt(_, _)
            ty::TyKind::Tuple(_)
            ty::TyKind::Never
            ty::TyKind::Param(_),
            */
        } 
    }
}

