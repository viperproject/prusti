use std::collections::HashMap;
use std::convert::TryFrom;

use viper::silicon_counterexample::*;
use viper::VerificationError;
use prusti_interface::data::ProcedureDefId;
use prusti_common::vir::CfgMethod;
use crate::encoder::Encoder;
use crate::encoder::places::{Local, LocalVariableManager, Place};
use crate::encoder::counterexample::*;

use rustc_middle::mir;
use rustc_middle::ty::{self, Ty, AdtKind, AdtDef, TyCtxt};
use rustc_span::MultiSpan;

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

        println!("var_debug_info: {:?}", var_debug_info);

        //optimally (at a later stage) we would use the "main" counterexample 
        //from silicon, the one not associated with any label, because it contains
        //the values of the function when it fails. But currently
        //most values can not be obtained there because they're folded
        let last_label = silicon_ce.label_order.last();
        
        //to be processed:
        let mut args_to_process: Vec<(String, MultiSpan, String, Ty)> = vec![];
        let mut entries_to_process: Vec<(String, MultiSpan, String, Ty)> = vec![];

        for vdi in var_debug_info{
            let rust_name = vdi.name.to_ident_string();
            let span = vdi.source_info.span;
            let multi_span = MultiSpan::from_span(span);
            let local: mir::Local = if let mir::VarDebugInfoContents::Place(place) = vdi.value {
                if let Some(local) = place.as_local() {
                    local
                } else {
                    continue;
                }
            } else {
                continue;
            };
            let index = local.index();
            let var_local = Local::from(local);
            let typ = local_variable_manager.get_type(var_local);
            let vir_name = local_variable_manager.get_name(var_local);
            entries_to_process.push((rust_name.clone(), multi_span.clone(), vir_name.clone(), typ));
            
            //if index indicates it is an argument
            if index > 0 && index <= arg_count {
                args_to_process.push((rust_name, multi_span, vir_name, typ))
            }
        }

        //for the return type:
        let return_local = Local::from(mir::Local::from_usize(0));
        //make sure
        let result_to_process = if local_variable_manager.is_return(return_local){
            //open question: what would be the right span for return type? None?
            let rust_name = String::from("result");
            let vir_name = if !is_pure {
                local_variable_manager.get_name(return_local)
            } else {
                String::from("Result()")
            };
            let typ = local_variable_manager.get_type(return_local);
            Some((rust_name, vir_name, typ))
        } else {
            //this case should probably never occur
            None
        };
        

        //now map those needed:
        let mut entries = HashMap::new();
        let mut args = HashMap::new();
        let result = if !is_pure {
            for (rust_name, span, vir_name, typ) in args_to_process {
                let opt_entry = silicon_ce.get_entry_at_label(&vir_name, Some(&String::from("l0")));
                let entry = backtranslate_entry(typ, opt_entry, tcx);
                args.insert((rust_name, span), entry);
            }
            for (rust_name, span, vir_name, typ) in entries_to_process {
                let opt_entry = silicon_ce.get_entry_at_label(&vir_name, last_label);
                let entry = backtranslate_entry(typ, opt_entry, tcx);
                entries.insert((rust_name, span), entry);
            }
            match result_to_process {
                None => Entry::Unit,
                Some((rust_name, vir_name, typ)) => {
                    let opt_entry = silicon_ce.get_entry_at_label(&vir_name, last_label);
                    println!("result entry: {:?}", opt_entry);
                    backtranslate_entry(typ, opt_entry, tcx)
                }
            }
        } else {
            for (rust_name, span, vir_name, typ) in args_to_process {
                let opt_entry = silicon_ce.get_entry_at_label(&vir_name, None);
                let entry = backtranslate_entry(typ, opt_entry, tcx);
                args.insert((rust_name, span), entry);
            }
            match result_to_process {
                None => Entry::Unit,
                Some((rust_name, vir_name, typ)) => {
                    let result_vir_name = String::from("Result()");
                    let opt_entry = silicon_ce.get_entry_at_label(&result_vir_name, None);
                    backtranslate_entry(typ, opt_entry, tcx)
                }
            }
        };
        
        Counterexample::Success {result, args, entries}
    } else {
        Counterexample::Failure(String::from("no counterexample generated"))
    }
}



fn backtranslate_entry<'tcx>(typ: Ty<'tcx>, sil_entry: Option<&ModelEntry>, tcx: &TyCtxt<'tcx>) -> Entry {
    //TODO: even if there is no matching entry, we might want some information.
    match typ.kind(){
        ty::TyKind::Bool => {
            match sil_entry{
                Some(ModelEntry::LitBoolEntry(value)) => Entry::BoolEntry { value: *value },
                Some(ModelEntry::RefEntry(name, map)) => {
                    let entry = map.get(&String::from("val_bool"));
                    if let Some(ModelEntry::LitBoolEntry(value)) = entry {
                        Entry::BoolEntry {value: *value}
                    } else {
                        Entry::UnknownEntry
                    }   
                },
                _ => Entry::UnknownEntry
            }
        },
        ty::TyKind::Int(_) | ty::TyKind::Uint(_) => {
            match sil_entry{
                Some(ModelEntry::LitIntEntry(value)) => Entry::IntEntry{ value: *value },
                Some(ModelEntry::RefEntry(name, map)) => {
                    let entry = map.get(&String::from("val_int"));
                    if let Some(ModelEntry::LitIntEntry(value)) = entry {
                        Entry::IntEntry{ value: *value }
                    } else { 
                        Entry::UnknownEntry
                    }
                }
                _ => Entry::UnknownEntry
            }
        },
        ty::TyKind::Char => {
            match sil_entry{
                Some(ModelEntry::LitIntEntry(value)) => {
                    let val_t = u8::try_from(*value).ok();
                    if let Some(v) = val_t {
                        return Entry::CharEntry{ value: v as char}
                    }
                },
                Some(ModelEntry::RefEntry(name, map)) => {
                    let entry = map.get(&String::from("val_int"));
                    if let Some(ModelEntry::LitIntEntry(value)) = entry {
                        let val_t = u8::try_from(*value);
                        if let Ok(v) = val_t {
                            return Entry::CharEntry{ value: v as char}
                        }
                    }
                },
                _ => ()
            }
            return Entry::UnknownEntry
        },
        ty::TyKind::Ref(_, typ, _) => {
            if let Some(ModelEntry::RefEntry(name, map)) = sil_entry {
                let entry = map.get(&String::from("val_ref"));
                let rec_entry = backtranslate_entry(typ, entry, tcx);
                Entry::RefEntry {el: Box::new(rec_entry)} 
            } else {
                Entry::RefEntry {el: Box::new(Entry::UnknownEntry)}
            }
        },
        ty::TyKind::Tuple(subst) => {
            let len = subst.types().count();
            if len > 0 {
                let mut fields = vec![];
                if let Some(ModelEntry::RefEntry(name, map)) = sil_entry {
                    for i in 0..len{
                        let typ = subst.type_at(i);
                        let field_id = format!("tuple_{}", i).to_string();
                        let field_entry = map.get(&field_id);
                        let rec_entry = backtranslate_entry(typ, field_entry, tcx);
                        fields.push(rec_entry);
                    }
                    Entry::Tuple{fields}
                } else {
                    Entry::UnknownEntry
                }
            } else {
                //Tuple with 0 fields is return type of functions with no
                //return value
                Entry::Unit
            }
        },
        ty::TyKind::Adt(adt_def, subst) => {
            match adt_def.adt_kind() {
                AdtKind::Struct => {
                    let variant = adt_def.variants.iter().next().unwrap();
                    let struct_name = variant.ident.name.to_ident_string();
                    let fields = &variant.fields;
                    
                    let (field_names, field_entries) = backtranslate_vardef(
                        variant,
                        sil_entry,
                        tcx,
                        subst,
                    );
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
                    if let Some(ModelEntry::RefEntry(_, map)) = sil_entry {
                        let disc_string = String::from("discriminant");
                        let mut variant = None;
                        let disc_entry = map.get(&disc_string);

                        //need to find a discriminant to do something
                        if let Some(&ModelEntry::LitIntEntry(x)) = disc_entry {
                            let discriminant = x as u32;
                            //is this a risk? I assume discriminant will not be too large
                            for def in &adt_def.variants{
                                let discr = match def.discr {
                                    ty::VariantDiscr::Relative(d) => d,
                                    _ => unimplemented!()  
                                };
                                if discr == discriminant {
                                    variant = Some(def);
                                    variant_name = def.ident.name.to_ident_string();
                                }
                            }
                        }

                        if let Some(var_def) = variant {
                            let sil_name = format!("enum_{}", variant_name).to_string();
                            let opt_enum_entry = map.get(&sil_name);
                            //at this point it should be a subroutine same for structs and enum:
                            let result = backtranslate_vardef(
                                var_def,
                                opt_enum_entry,
                                tcx,
                                subst
                            );
                            field_names = result.0;
                            field_entries = result.1;
                        }
                    }
                    //valid names can not start with integers (?)
                    let named_fields = field_names.len() > 0 && !field_names[0].parse::<usize>().is_ok();

                    Entry::Enum{
                        super_name,
                        name: variant_name,
                        named_fields,
                        field_names,
                        field_entries
                    }
                },
                    //afaik unions are not supported
                _ => unimplemented!()
            }
        },
        _ => Entry::UnknownEntry
    }  
}

fn backtranslate_vardef<'tcx>(
    variant: &ty::VariantDef, 
    sil_entry: Option<&ModelEntry>, 
    tcx: &TyCtxt<'tcx>,
    subst: ty::subst::SubstsRef<'tcx>,
) -> (Vec<String>, Vec<Entry>) {
    let mut field_names = vec![];
    let mut field_entries = vec![];
    if let Some(ModelEntry::RefEntry(name, map)) = sil_entry {
        for f in &variant.fields {
            let field_name = f.ident.name.to_ident_string();
            let typ = f.ty(*tcx, subst);
            //extract recursively:
            let sil_name = format!("f${}", field_name).to_string();
            let rec_entry = map.get(&sil_name);
            let field_entry = match rec_entry {
                Some(ModelEntry::RecursiveRefEntry(refname)) => {
                    assert!(refname == name);
                    backtranslate_entry(typ, sil_entry, tcx)
                },
                _ => backtranslate_entry(typ, rec_entry, tcx),
            };
            field_names.push(field_name);
            field_entries.push(field_entry);
        }
    }
    (field_names, field_entries)
}