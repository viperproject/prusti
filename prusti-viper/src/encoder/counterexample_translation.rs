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

use rustc_middle::mir::{self, VarDebugInfo};
use rustc_middle::ty::{self, Ty, AdtKind, AdtDef, TyCtxt};
use rustc_span::Span;


pub struct CounterexampleTranslator<'tcx> {
    mir: mir::Body<'tcx>,
    def_id: ProcedureDefId,
    silicon_counterexample: SiliconCounterexample,
    tcx: TyCtxt<'tcx>,
    is_pure: bool,
    disc_info: HashMap<(ProcedureDefId, String), Vec<String>>,
    var_debug_info: Vec<VarDebugInfo<'tcx>>,
    local_variable_manager: LocalVariableManager<'tcx>,
}



pub fn backtranslate<'tcx>(
    encoder: &Encoder,
    def_id: ProcedureDefId,
    silicon_counterexample: SiliconCounterexample,
) -> Option<Counterexample> {
    let translator = CounterexampleTranslator::new(encoder, def_id, silicon_counterexample);
    //optimally (at a later stage) we would use the "main" counterexample 
    //from silicon, the one not associated with any label, because it contains
    //the values of the function when it fails. But currently
    //most values can not be obtained there because they're folded
    let last_label: Option<&String> = translator
        .silicon_counterexample
        .label_order.last();
    
    let old_impure_label = if translator
        .silicon_counterexample
        .label_order
        .contains(&"l0".to_string()) {
            "l0".to_string()
        } else {
            "old".to_string()
        };
    let old_label = if translator.is_pure {
        None
    } else {
        Some(&old_impure_label)
    };
    //to be processed:
    let entries_to_process = translator.entries_to_process();
    let (result_sil_name, result_typ) = translator.result_to_process();
    //now map those needed:
    let mut entries = HashMap::new();
    let mut args = HashMap::new();

    for (rust_name, span, vir_name, typ, is_arg) in entries_to_process {
        if !translator.is_pure {
            let entry = translator.process_variable_at_label(last_label, &vir_name, typ);
            entries.insert((rust_name.clone(), span.clone()), entry);
        }
        if is_arg {
            let arg_entry = translator.process_variable_at_label(old_label, &vir_name, typ);
            args.insert((rust_name, span), arg_entry);
        }
    }
    let result = translator.process_variable_at_label(
        last_label,
        &result_sil_name,
        result_typ,
    );
    let result_span = translator.result_span();

    Some(Counterexample::new(result, result_span, args, entries, translator.is_pure))
}

impl<'tcx> CounterexampleTranslator<'tcx> {
    pub fn new(
        encoder: &Encoder<'_, 'tcx>,
        def_id: ProcedureDefId,
        silicon_counterexample: SiliconCounterexample,
    ) -> CounterexampleTranslator<'tcx> {
        let mir = encoder.env().get_procedure(def_id).get_mir().clone();
        let tcx = encoder.env().tcx();
        let is_pure = encoder.is_pure(def_id);
        let disc_info = encoder.discriminants_info();
        let var_debug_info = mir.var_debug_info.clone();
        let local_variable_manager = LocalVariableManager::new(&mir.local_decls);
        CounterexampleTranslator {
            mir,
            def_id,
            silicon_counterexample,
            tcx,
            is_pure,
            disc_info,
            var_debug_info,
            local_variable_manager,
        }
    }

    fn entries_to_process(&self) -> Vec<(String, Span, String, Ty<'tcx>, bool)> {
        let mut entries_to_process = vec![];
        for vdi in &self.var_debug_info {
            let rust_name = vdi.name.to_ident_string();
            let span = vdi.source_info.span;
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
            let typ = self.local_variable_manager.get_type(var_local);
            let is_arg = index > 0 && index <= self.mir.arg_count;
            let vir_name = self.local_variable_manager.get_name(var_local);
            entries_to_process.push((rust_name.clone(), span.clone(), vir_name.clone(), typ, is_arg));
        }
        entries_to_process
    }
    
    fn result_to_process(&self) -> (String, Ty<'tcx>) {
        // return the Silicon-name of result + its type
        // other than for entries, this is not always same as the vir-name
        let return_local = Local::from(mir::Local::from_usize(0));

        if self.local_variable_manager.is_return(return_local) {
            //open question: what would be the right span for return type? None?
            let vir_name = if !self.is_pure {
                self.local_variable_manager.get_name(return_local)
            } else {
                "Result()".to_string()
            };
            let typ = self.local_variable_manager.get_type(return_local);
            (vir_name, typ)
        } else {
            //this case should probably never occur
            unreachable!(); 
        }
    }

    fn result_span(&self) -> Option<Span> {
        //figure out span of result
        let hir = self.tcx.hir();
        let hir_id = hir.local_def_id_to_hir_id(self.def_id.as_local().unwrap());
        hir.fn_decl_by_hir_id(hir_id).and_then(|x| Some(x.output.span()))
    }

    fn process_variable_at_label(
        &self, 
        label: Option<&String>, 
        var_name: &String,
        typ: Ty<'tcx>,
    ) -> Entry {
        //process a list of variables at a certain label
        let silicon_model = match label {
            // expect to only be called on labels that actually exist (?)
            Some(lbl) => &self.silicon_counterexample.old_models.get(lbl).unwrap().entries,
            None => &self.silicon_counterexample.model.entries,
        };
        let opt_sil_entry = silicon_model.get(var_name);
        self.translate_silicon_entry(
            typ,
            opt_sil_entry,
            var_name.to_string(),
            silicon_model,
        )
    }
    
    fn translate_silicon_entry(
        &self,
        typ: Ty<'tcx>, 
        sil_entry: Option<&ModelEntry>, 
        vir_name: String,
        silicon_ce_entries: &HashMap<String, ModelEntry>,
    ) -> Entry {
        match typ.kind(){
            ty::TyKind::Bool => {
                match sil_entry {
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
                let opt_value = self.translate_int(sil_entry);
                if let Some(value) = opt_value {
                    Entry::IntEntry { value }
                } else {
                    Entry::UnknownEntry
                }
            },
            ty::TyKind::Char => {
                let opt_value = self.translate_int(sil_entry);
                if let Some(value) = opt_value {
                    let val_t = u32::try_from(value);
                    if let Ok(v) = val_t {
                        let opt_char = char::from_u32(v);
                        if let Some(chr) = opt_char {
                            return Entry::CharEntry{ value: chr }
                        }
                    } 
                }
                Entry::UnknownEntry
            },
            ty::TyKind::Ref(_, typ, _) => {
                if let Some(ModelEntry::RefEntry(name, map)) = sil_entry {
                    let entry = map.get("val_ref");
                    let new_vir_name = format!("{}.val_ref", vir_name);
                    let rec_entry = self.translate_silicon_entry(
                        typ, 
                        entry, 
                        new_vir_name,
                        silicon_ce_entries, 
                    );
                    Entry::RefEntry { el: box rec_entry } 
                } else {
                    Entry::RefEntry { el: box Entry::UnknownEntry }
                }
            },
            ty::TyKind::Tuple(subst) => {
                let len = subst.types().count();
                if len > 0 {
                    let mut fields = vec![];
                    if let Some(ModelEntry::RefEntry(name, map)) = sil_entry {
                        for i in 0..len {
                            let typ = subst.type_at(i);
                            let field_id = format!("tuple_{}", i);
                            let new_vir_name = format!("{}.{}", vir_name, field_id);
                            let field_entry = map.get(&field_id);
                            let rec_entry = self.translate_silicon_entry(
                                typ, 
                                field_entry,
                                new_vir_name,
                                silicon_ce_entries,
                            );
                            fields.push(rec_entry);
                        }
                        Entry::Tuple{ fields }
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
                        
                        let field_entries = self.translate_vardef(
                            variant,
                            sil_entry,
                            vir_name,
                            subst,
                            silicon_ce_entries,
                        );
                        Entry::Struct{
                            name: struct_name,
                            field_entries,
                        }
                    },
                    AdtKind::Enum => {
                        let super_name = format!("{:?}", adt_def);
                        let mut variant_name = "?".to_string();
                        let mut field_entries = vec![];
                        if let Some(ModelEntry::RefEntry(name, map)) = sil_entry {
                            let mut variant = None;
                            let mut opt_discriminant = self.translate_int(map.get("discriminant"));
                            //need to find a discriminant to do something
                            if !opt_discriminant.is_some() {
                                //try to find disc in the associated local variable
                                let opt_discr_locations = self.disc_info.get(&(self.def_id, vir_name.clone()));
                                if let Some(discr_locations) = opt_discr_locations {
                                    for name in discr_locations {
                                        let disc_entry = silicon_ce_entries.get(name);
                                        let value = self.translate_int(disc_entry);
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
                                variant = adt_def.variants.iter()
                                    .filter(|x| get_discriminant_of_vardef(x) == Some(discriminant))
                                    .next();
                                if let Some(v) = variant {
                                    variant_name = v.ident.name.to_ident_string();
                                }
                            }
                            
                            if let Some(var_def) = variant {
                                let sil_name = format!("enum_{}", variant_name);
                                let opt_enum_entry = map.get(&sil_name);
                                //at this point it should be a subroutine same for structs and enum:
                                field_entries = self.translate_vardef(
                                    var_def,
                                    opt_enum_entry,
                                    vir_name,
                                    subst,
                                    silicon_ce_entries,
                                );
                            }
                        }

                        Entry::Enum{
                            super_name,
                            name: variant_name,
                            field_entries,
                        }
                    },
                        //afaik unions are not supported
                    _ => unreachable!(),
                }
            },
            _ => Entry::UnknownEntry
        }  
    }

    fn translate_vardef(
        &self,
        variant: &ty::VariantDef, 
        sil_entry: Option<&ModelEntry>,
        vir_name: String,
        subst: ty::subst::SubstsRef<'tcx>,
        silicon_ce_entries: &HashMap<String, ModelEntry>,
    ) -> Vec<(String, Entry)> {
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
                        self.translate_silicon_entry(
                            typ, 
                            real_ref_entry, 
                            new_vir_name,
                            silicon_ce_entries,
                        )
                    },
                    _ => self.translate_silicon_entry(
                        typ,
                        rec_entry,
                        new_vir_name,
                        silicon_ce_entries,
                    ),
                };
            }
            field_entries.push((field_name, field_entry));
        }
        field_entries
    }

    fn translate_int(&self, opt_sil_entry: Option<&ModelEntry>) -> Option<i64> {
        match opt_sil_entry {
            Some(ModelEntry::LitIntEntry(value)) => Some(*value),
            Some(ModelEntry::RefEntry(name, map)) => {
                let entry = map.get("val_int");
                if let Some(ModelEntry::LitIntEntry(value)) = entry {
                    Some(*value)
                } else { 
                    None
                }
            },
            _ => None,
        }
    }
}

fn get_discriminant_of_vardef(vardef: &ty::VariantDef) -> Option<u32> {
    match vardef.discr {
        ty::VariantDiscr::Relative(x) => Some(x),
        _ => None,
    }
}