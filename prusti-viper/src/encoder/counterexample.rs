use std::collections::HashMap;
use std::fmt;


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
        match self {
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

impl fmt::Display for Counterexample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Counterexample::Success{result, args, entries} => {
                write!(f, "Counterexample:\n");
                write!(f, "initial args:\n");
                for (name, entry) in args {
                    write!(f, "{} <- {}\n", name, entry);
                }
                write!(f, "\nlocal values when failing:\n");
                for (name, entry) in entries {
                    write!(f, "{} <- {}\n", name, entry);
                }
                write!(f, "\nresult <- {}", result)
            },
            _ => write!(f, "Counterexample generation failed")
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



