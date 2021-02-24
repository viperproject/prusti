use std::collections::HashMap;
use std::hash::{Hasher, Hash};

use jni::JNIEnv;
use jni::objects::JObject;
use jni_utils::JniUtils;
use viper_sys::wrappers::viper::silicon;
use viper_sys::wrappers::scala;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiliconCounterexample{
    pub heap: Heap,
    pub old_heaps: HashMap<String, Heap>,
    pub model: Model,
    pub old_models: HashMap<String, Model>,
    hash_helper: String 
    //this is part of a very ugly way to make this implement Hash and Eq
}

impl SiliconCounterexample{
    pub fn new<'a>(
        env: &'a JNIEnv<'a>,
        jni: JniUtils<'a>,
        counterexample: JObject<'a>
    ) -> SiliconCounterexample {
        unwrap_counterexample(env, jni, counterexample)
    }

    pub fn get_entry_at_label(&self, name: &String, label: Option<&String>) -> Option<&ModelEntry>{
        match label{
            Some(s) => {
                let model = self.old_models.get(s);
                match model{
                    Some(m) => m.entries.get(name),
                    None => None
                }
            },
            None => {
                self.model.entries.get(name)
            }
        }
    }

    pub fn get_last_label(&self) -> Option<&String>{
        //look for the highest l* label
        let mut max = -1;
        let mut lbl = None;
        for label in self.old_models.keys() {
            let maybe_number = label[1..].to_string();
            let index = maybe_number.parse::<i32>();
            if let Ok(i) = index{
                if i > max {
                    max = i;
                    lbl = Some(label);
                }
            }
        }
        lbl
    }
}

impl PartialEq for SiliconCounterexample{
    fn eq(&self, other: &Self) -> bool{
        self.hash_helper == other.hash_helper
    }
}
impl Eq for SiliconCounterexample{}

impl Hash for SiliconCounterexample{
    fn hash<H:Hasher>(&self, state: &mut H){
        self.hash_helper.hash(state);
    }
}

//Heap Definitions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Heap{
    pub entries: Vec<HeapEntry>
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HeapEntry{
    FieldEntry{
        recv: ModelEntry,
        //perm & sort omitted
        field: String,
        entry: ModelEntry
    },
    PredicateEntry{
        name: String,
        args: Vec<ModelEntry> 
    }
}

//Model Definitions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model{
    pub entries: HashMap<String, ModelEntry>
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ModelEntry{
    LitIntEntry(i64),
    LitBoolEntry(bool),
    LitPermEntry(f64),
    RefEntry(String, HashMap<String, ModelEntry>),  //might need to add permission here
    NullRefEntry(String),
    RecursiveRefEntry(String),
    VarEntry(String),
    SeqEntry(String, Vec<ModelEntry>), //not necessarily supported
    OtherEntry(String, String),
    UnprocessedModelEntry //do not use these at all
}

//methods unwrapping scala converter to newly defined structures:

fn unwrap_counterexample<'a>(
    env: &'a JNIEnv<'a>, 
    jni: JniUtils<'a>, 
    counterexample: JObject<'a>)
    -> SiliconCounterexample
{
    let converter_wrapper = silicon::reporting::Converter::with(env);
    let ce_wrapper = silicon::interfaces::SiliconMappedCounterexample::with(env);
    //1. get converter from counterexample
        
    let converter_original = jni
        .unwrap_result(
            ce_wrapper
            .call_converter(counterexample)
        );


    let heap_scala = jni.
        unwrap_result(
            converter_wrapper
                .call_extractedHeap(converter_original)
            );
    let heap = unwrap_heap(env, jni, heap_scala);


    let old_heaps_scala = jni
        .unwrap_result(
            converter_wrapper
            .call_extractedHeaps(converter_original)
        );
    let old_heaps_map = jni.stringmap_to_hashmap(old_heaps_scala);
    let mut old_heaps = HashMap::new();
    for (label, h) in old_heaps_map {
        let old_heap = unwrap_heap(env, jni, h);
        old_heaps.insert(label, old_heap);
    }


 
    let model_scala = jni
        .unwrap_result(
            converter_wrapper
            .call_extractedModel(converter_original)
        );
    let model = unwrap_model(env, jni, model_scala);


    let old_models_scala = jni
        .unwrap_result(
            converter_wrapper
            .call_modelAtLabel(converter_original)
        );
    let old_models_map = jni.stringmap_to_hashmap(old_models_scala);
    let mut old_models = HashMap::new();
    for (label, m) in old_models_map{
        let old_model = unwrap_model(env, jni, m);
        old_models.insert(label, old_model);
    }
    let hash_helper = "hashme".to_string();
    SiliconCounterexample{
        heap,
        old_heaps,
        model,
        old_models,
        hash_helper
    }
} 



fn unwrap_heap<'a>(env: &'a JNIEnv<'a>, jni: JniUtils<'a>, heap: JObject<'a>) -> Heap {
    let heap_wrapper = silicon::reporting::ExtractedHeap::with(env);
    let entries_scala = jni
        .unwrap_result(
        heap_wrapper.call_entries(heap)
    ); // List[HeapEntries] in scala
    let entries_vec = jni.list_to_vec(entries_scala);
    let mut entries = vec![];
    for i in 0..(entries_vec.len()) {
        let entry = unwrap_heap_entry(env, jni, entries_vec[i]);
        match entry{
            Some(e) => entries.push(e),
            None => ()
        };
    }
    Heap{
        entries 
    }
}

fn unwrap_heap_entry<'a>(
    env: &'a JNIEnv<'a>, 
    jni: JniUtils<'a>, 
    entry: JObject<'a>
) -> Option<HeapEntry> {
    //we can ignore FieldEntries since the ones that are interesting should already be in model
    //let field_entry_wrapper = silicon::reporting::FieldHeapEntry::with(env);
    let predicate_entry_wrapper = silicon::reporting::PredHeapEntry::with(env);

    if jni.is_instance_of(entry, "viper/silicon/reporting/PredHeapEntry"){
        //get fields:
        let name_scala = jni.unwrap_result(predicate_entry_wrapper.call_name(entry));
        let name = jni.to_string(name_scala);

        let args_scala = jni
            .unwrap_result(
                predicate_entry_wrapper
                .call_args(entry)
            );
        let args_vec = jni.seq_to_vec(args_scala);
        let mut args = vec![];
        for el in args_vec{
            let me = unwrap_model_entry(env, jni, el);
            match me{
                Some(e) => args.push(e),
                None => ()
            }
        }

        let res = HeapEntry::PredicateEntry{
            name,
            args
        };
        Some(res)
    } else {
        //extend for FieldEntries if needed
        None
    }
}


fn unwrap_model<'a>(
    env: &'a JNIEnv<'a>,
    jni: JniUtils<'a>,
    model: JObject<'a>
) -> Model {
    let model_wrapper = silicon::reporting::ExtractedModel::with(env);
    let entries_scala = jni.unwrap_result(model_wrapper.call_entries(model));    
    let map_string_scala = jni.stringmap_to_hashmap(entries_scala);

    let mut entries = HashMap::new();
    for (name, entry_scala) in map_string_scala {
        let entry = unwrap_model_entry(env, jni, entry_scala);
        match entry{
            Some(e) => {entries.insert(name, e);},
            None => (),
        };
    }
    Model{entries}
}

fn unwrap_model_entry<'a>(
    env: &'a JNIEnv<'a>,
    jni: JniUtils<'a>,
    entry: JObject<'a>
) -> Option<ModelEntry> {
    // either check is_instance_of for every possible class or 
    // get classname and match strings. First one feels saver, maybe more expensive
    if jni.is_instance_of(entry, "viper/silicon/reporting/LitIntEntry"){
        //one weakness: in scala value is BigInt
        let litint_wrapper = silicon::reporting::LitIntEntry::with(env);
        let value_scala = jni.unwrap_result(litint_wrapper.call_value(entry));
        let value_str = jni.to_string(value_scala);
        let value = value_str.parse::<i64>().unwrap();
        Some(ModelEntry::LitIntEntry(value))
    } else if jni.is_instance_of(entry, "viper/silicon/reporting/LitBoolEntry"){
        let lit_bool_wrapper = silicon::reporting::LitBoolEntry::with(env);
        let value = jni.unwrap_result(lit_bool_wrapper.call_value(entry));
        Some(ModelEntry::LitBoolEntry(value))
    } else if jni.is_instance_of(entry, "viper/silicon/reporting/LitPermEntry"){
        let lit_perm_wrapper = silicon::reporting::LitPermEntry::with(env);
        let value = jni.unwrap_result(lit_perm_wrapper.call_value(entry));
        Some(ModelEntry::LitPermEntry(value))
    } else if jni.is_instance_of(entry, "viper/silicon/reporting/RefEntry"){
        let ref_wrapper = silicon::reporting::RefEntry::with(env);
        let product_wrapper = scala::Product::with(env);
        
        let mut result = HashMap::new();
        //get name
        let name_scala = jni.unwrap_result(ref_wrapper.call_name(entry));
        let fields_scala = jni.unwrap_result(ref_wrapper.call_fields(entry));
        let name = jni.to_string(name_scala);
        //get list of fields
        let fields_map_scala = jni.stringmap_to_hashmap(fields_scala);
        for (field, tuple_scala) in fields_map_scala {
            let element_scala = jni.unwrap_result(product_wrapper.call_productElement(tuple_scala, 0));
            let element = unwrap_model_entry(env, jni, element_scala);
            match element{
                Some(e) => {result.insert(field, e);},
                None => ()
            }
        }
        Some(ModelEntry::RefEntry(name, result))
    } else if jni.is_instance_of(entry, "viper/silicon/reporting/NullRefEntry"){
        let null_ref_wrapper = silicon::reporting::NullRefEntry::with(env);
        let name = jni.to_string(
            jni.unwrap_result(null_ref_wrapper.call_name(entry))
        );
        Some(ModelEntry::NullRefEntry(name))
    } else if jni.is_instance_of(entry, "viper/silicon/reporting/RecursiveRefEntry"){
        let rec_ref_wrapper = silicon::reporting::RecursiveRefEntry::with(env);
        let name = jni.to_string(
            jni.unwrap_result(rec_ref_wrapper.call_name(entry))
        );
        Some(ModelEntry::RecursiveRefEntry(name))
    } else if jni.is_instance_of(entry, "viper/silicon/reporting/VarEntry"){
        let var_wrapper = silicon::reporting::VarEntry::with(env);
        let name = jni.to_string(
            jni.unwrap_result(var_wrapper.call_name(entry))
        );
        Some(ModelEntry::VarEntry(name))
    } else if jni.is_instance_of(entry, "viper/silicon/reporting/OtherEntry"){
        let other_entry_wrapper = silicon::reporting::OtherEntry::with(env);
        let value = jni.to_string(
            jni.unwrap_result(other_entry_wrapper.call_value(entry))
        );
        let problem = jni.to_string(
            jni.unwrap_result(other_entry_wrapper.call_problem(entry))
        );
        Some(ModelEntry::OtherEntry(value, problem))

    } else if jni.is_instance_of(entry, "viper/silicon/reporting/SeqEntry"){
        let seq_entry_wrapper = silicon::reporting::SeqEntry::with(env);
        let name = jni.to_string(
            jni.unwrap_result(seq_entry_wrapper.call_name(entry))
        );
        let list_scala = jni.unwrap_result(seq_entry_wrapper.call_values(entry));
        let vec_scala = jni.list_to_vec(list_scala);
        let mut res = vec![];
        for el in vec_scala{
            let element = unwrap_model_entry(env, jni, el);
            match element {
                Some(e) => res.push(e),
                None => (),
            }
        }
        Some(ModelEntry::SeqEntry(name, res))
    } else {
        None
    }
}