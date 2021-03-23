use std::collections::HashMap;
use std::convert::TryFrom;
use std::char;

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


pub struct CounterexampleTranslator<'tcx> {
    mir: mir::Body<'tcx>,
    def_id: ProcedureDefId,
    silicon_counterexample: SiliconCounterexample,
    tcx: TyCtxt<'tcx>,
    is_pure: bool,
    disc_info: HashMap<(ProcedureDefId, String), Vec<String>>,
}

impl<'tcx> CounterexampleTranslator<'tcx> {
    pub fn new(
        encoder: &Encoder<'_, 'tcx>,
        def_id: ProcedureDefId,
        silicon_counterexample: SiliconCounterexample
    ) -> CounterexampleTranslator<'tcx> {
        let mir = encoder.env().get_procedure(def_id).get_mir().clone();
        let tcx = encoder.env().tcx();
        let is_pure = encoder.is_pure(def_id);
        let disc_info = encoder.discriminants_info();
        CounterexampleTranslator {
            mir,
            def_id,
            silicon_counterexample,
            tcx,
            is_pure,
            disc_info,
        }
    }
}

pub fn backtranslate<'tcx>(
    encoder: &Encoder,
    def_id: ProcedureDefId,
    opt_silicon_counterexample: Option<SiliconCounterexample>,
) -> Option<Counterexample> {
    if let Some(silicon_counterexample) = opt_silicon_counterexample {
        let backtranslator = CounterexampleTranslator::new(encoder, def_id, silicon_counterexample);
        //get all the needed information from the mir
        let var_debug_info = backtranslator.mir.var_debug_info.clone();
        let local_variable_manager = LocalVariableManager::new(&backtranslator.mir.local_decls);
        let arg_count = backtranslator.mir.arg_count;

        //optimally (at a later stage) we would use the "main" counterexample 
        //from silicon, the one not associated with any label, because it contains
        //the values of the function when it fails. But currently
        //most values can not be obtained there because they're folded
        let last_label: Option<&String> = backtranslator
            .silicon_counterexample
            .label_order.last();
        //optimally this label would just be "old", but as of now mostly the values
        //are not available at this point
        let old_label = "l0".to_string();

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
            let rust_name = "result".to_string();
            let vir_name = if !backtranslator.is_pure {
                local_variable_manager.get_name(return_local)
            } else {
                "Result()".to_string()
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

        let result = if !backtranslator.is_pure {
            if backtranslator.silicon_counterexample.old_models.contains_key(&old_label){ 
                let silicon_arg_entries = &backtranslator.silicon_counterexample.old_models
                    .get(&old_label)
                    .unwrap()
                    .entries;
                for (rust_name, span, vir_name, typ) in args_to_process {
                    let opt_entry = silicon_arg_entries.get(&vir_name);
                    let entry = backtranslator.backtranslate_entry(
                        typ, 
                        opt_entry,
                        vir_name,
                        silicon_arg_entries,
                    );
                    args.insert((rust_name, span), entry);
                }
            }
            let silicon_final_entries = if last_label.is_some() {
                &backtranslator.silicon_counterexample.old_models
                    .get(last_label.unwrap())
                    .unwrap()
                    .entries
                } else {
                    &backtranslator.silicon_counterexample.model.entries 
                };
            for (rust_name, span, vir_name, typ) in entries_to_process {
                let opt_entry = silicon_final_entries.get(&vir_name);
                let entry = backtranslator.backtranslate_entry(
                    typ,
                    opt_entry, 
                    vir_name,
                    silicon_final_entries,
                );
                entries.insert((rust_name, span), entry);
            }
            match result_to_process {
                None => Entry::Unit,
                Some((rust_name, vir_name, typ)) => {
                    let opt_entry = silicon_final_entries.get(&vir_name);
                    backtranslator.backtranslate_entry(
                        typ, 
                        opt_entry, 
                        vir_name,
                        silicon_final_entries,
                    )
                }
            }
        } else {
            let silicon_entries = &backtranslator.silicon_counterexample.model.entries;
            for (rust_name, span, vir_name, typ) in args_to_process {
                let opt_entry = silicon_entries.get(&vir_name);
                let entry = backtranslator.backtranslate_entry(
                    typ, 
                    opt_entry, 
                    vir_name,
                    silicon_entries,
                );
                args.insert((rust_name, span), entry);
            }
            match result_to_process {
                None => Entry::Unit,
                Some((rust_name, vir_name, typ)) => {
                    let result_vir_name = "Result()".to_string();
                    let opt_entry = silicon_entries.get(&result_vir_name);
                    backtranslator.backtranslate_entry(
                        typ, 
                        opt_entry, 
                        result_vir_name,
                        silicon_entries,
                    )
                }
            }
        };
        Some(Counterexample::new(result, args, entries, backtranslator.is_pure))
    } else {
        None
    }
}

impl<'tcx> CounterexampleTranslator<'tcx> {
    fn backtranslate_entry(
        &self,
        typ: Ty<'tcx>, 
        sil_entry: Option<&ModelEntry>, 
        vir_name: String,
        silicon_ce_entries: &HashMap<String, ModelEntry>,
    ) -> Entry {
        match typ.kind(){
            ty::TyKind::Bool => {
                match sil_entry{
                    Some(ModelEntry::LitBoolEntry(value)) => Entry::BoolEntry { value: *value },
                    Some(ModelEntry::RefEntry(name, map)) => {
                        let entry = map.get("val_bool");
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
                let opt_value = self.backtranslate_int(sil_entry);
                if let Some(value) = opt_value {
                    Entry::IntEntry { value }
                } else {
                    Entry::UnknownEntry
                }
            },
            ty::TyKind::Char => {
                let opt_value = self.backtranslate_int(sil_entry);
                if let Some(value) = opt_value {
                    let val_t = u32::try_from(value);
                    if let Ok(v) = val_t {
                        let opt_char = char::from_u32(v);
                        if let Some(chr) = opt_char {
                            return Entry::CharEntry{ value: chr}
                        }
                    } 
                }
                Entry::UnknownEntry
            },
            ty::TyKind::Ref(_, typ, _) => {
                if let Some(ModelEntry::RefEntry(name, map)) = sil_entry {
                    let entry = map.get("val_ref");
                    let new_vir_name = format!("{}.val_ref", vir_name);
                    let rec_entry = self.backtranslate_entry(
                        typ, 
                        entry, 
                        new_vir_name,
                        silicon_ce_entries, 
                    );
                    Entry::RefEntry {el: box rec_entry} 
                } else {
                    Entry::RefEntry {el: box Entry::UnknownEntry}
                }
            },
            ty::TyKind::Tuple(subst) => {
                let len = subst.types().count();
                if len > 0 {
                    let mut fields = vec![];
                    if let Some(ModelEntry::RefEntry(name, map)) = sil_entry {
                        for i in 0..len{
                            let typ = subst.type_at(i);
                            let field_id = format!("tuple_{}", i);
                            let new_vir_name = format!("{}.{}", vir_name, field_id);
                            let field_entry = map.get(&field_id);
                            let rec_entry = self.backtranslate_entry(
                                typ, 
                                field_entry,
                                new_vir_name,
                                silicon_ce_entries,
                            );
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
                        
                        let (field_names, field_entries) = self.backtranslate_vardef(
                            variant,
                            sil_entry,
                            vir_name,
                            subst,
                            silicon_ce_entries,
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
                        let mut variant_name = "?".to_string();
                        let mut field_names = vec![];
                        let mut field_entries = vec![];
                        if let Some(ModelEntry::RefEntry(name, map)) = sil_entry {
                            let mut variant = None;
                            let mut opt_discriminant = self.backtranslate_int(map.get("discriminant"));
                            //need to find a discriminant to do something
                            if !opt_discriminant.is_some(){
                                //try to find disc in the associated local variable
                                let opt_discr_locations = self.disc_info.get(&(self.def_id, vir_name.clone()));
                                if let Some(discr_locations) = opt_discr_locations {
                                    for name in discr_locations {
                                        let disc_entry = silicon_ce_entries.get(name);
                                        let value = self.backtranslate_int(disc_entry);
                                        if value.is_some() {
                                            opt_discriminant = value;
                                            break;
                                        }
                                    }
                                }
                            }
                            if let Some(x) = opt_discriminant {
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
                                let sil_name = format!("enum_{}", variant_name);
                                let opt_enum_entry = map.get(&sil_name);
                                //at this point it should be a subroutine same for structs and enum:
                                let result = self.backtranslate_vardef(
                                    var_def,
                                    opt_enum_entry,
                                    vir_name,
                                    subst,
                                    silicon_ce_entries,
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

    fn backtranslate_vardef(
        &self,
        variant: &ty::VariantDef, 
        sil_entry: Option<&ModelEntry>,
        vir_name: String,
        subst: ty::subst::SubstsRef<'tcx>,
        silicon_ce_entries: &HashMap<String, ModelEntry>,
    ) -> (Vec<String>, Vec<Entry>) {
        let mut field_names = vec![];
        let mut field_entries = vec![];
        for f in &variant.fields {
            let field_name = f.ident.name.to_ident_string();
            let typ = f.ty(self.tcx, subst);
            //extract recursively:
            let sil_name = format!("f${}", field_name);
            let new_vir_name = format!("{}.f${}", vir_name, field_name);
            let mut field_entry = Entry::UnknownEntry;

            if let Some(ModelEntry::RefEntry(name, map)) = sil_entry {
                let rec_entry = map.get(&sil_name);
                field_entry = match rec_entry {
                    Some(ModelEntry::RecursiveRefEntry(refname)) => {
                        //this unwrap should never fail unless
                        //there is a fault in silicon's implementation
                        let real_ref_entry = silicon_ce_entries.get(refname);
                        self.backtranslate_entry(
                            typ, 
                            real_ref_entry, 
                            new_vir_name,
                            silicon_ce_entries,
                        )
                    },
                    _ => self.backtranslate_entry(
                        typ,
                        rec_entry,
                        new_vir_name,
                        silicon_ce_entries,
                    ),
                };
            }
            field_names.push(field_name);
            field_entries.push(field_entry);
        }
        
        (field_names, field_entries)
    }

    fn backtranslate_int(&self, opt_sil_entry: Option<&ModelEntry>) -> Option<i64> {
        match opt_sil_entry {
            Some(ModelEntry::LitIntEntry(value)) => Some(*value),
            Some(ModelEntry::RefEntry(name, map)) => {
                let entry = map.get("val_int");
                if let Some(ModelEntry::LitIntEntry(value)) = entry {
                    Some(*value)
                } else { 
                    None
                }
            }
            _ => None
        }
    }
}