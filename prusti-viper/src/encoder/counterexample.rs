use std::collections::HashMap;
use std::fmt;
use rustc_span::Span;
use prusti_interface::PrustiError;

pub struct Counterexample {
    result: Entry,
    result_span: Option<Span>,
    args: HashMap<(String, Span), Entry>,
    entries: HashMap<(String, Span), Entry>,
    is_pure: bool,
}

impl Counterexample {
    pub fn new(
        result: Entry,
        result_span: Option<Span>,
        args: HashMap<(String, Span), Entry>,
        entries: HashMap<(String, Span), Entry>,
        is_pure: bool,
    ) -> Counterexample {
        Counterexample {
            result,
            result_span,
            args,
            entries,
            is_pure,
        }
    }

    pub fn annotate_error(&self, mut prusti_error: PrustiError) -> PrustiError {
        if !self.is_pure {
            for (place, entry) in &self.entries {
                //place is a tuple (Name of the variable, Option<Scope>)
                if let Some(entry_arg) = self.args.get(place) {
                    let note = format!("counterexample for \"{0}\"\ninitial: {0} <- {1:#?}\nfinal: {0} <- {2:#?}", 
                        place.0, 
                        entry_arg,  
                        entry,
                    );
                    prusti_error = prusti_error.add_note(&note, Some(place.1.clone()));
                } else {
                    let note = format!("counterexample for \"{0}\"\n{0} <- {1:#?}", place.0, entry);
                    prusti_error = prusti_error.add_note(&note, Some(place.1.clone()));
                }
            }
            let result_note = format!("result <- {:#?}", self.result);
            prusti_error = prusti_error.add_note(&result_note, self.result_span.clone());
        } else {
            for (place, entry) in &self.args {
                let note = format!("counterexample for \"{0}\"\n{0} <- {1:#?}", 
                        place.0,   
                        entry,
                    );
                prusti_error = prusti_error.add_note(&note, Some(place.1.clone()));
            }
            // Todo: find span of return type to give this note a span
            let result_note = format!("counterexample for result \nresult <- {:#?}", self.result);
            prusti_error = prusti_error.add_note(&result_note, None);
        }
        prusti_error
    }
}

impl fmt::Display for Counterexample {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Counterexample:\n");
        write!(f, "initial args:\n");
        for (place, entry) in &self.args {
            let s = format!("{} <- {:?}\n", place.0, entry);
            write!(f, "{}", indent(s));
        }
        write!(f, "\nlocal values when failing:\n");
        for (place, entry) in &self.entries {
            let s = format!("{} <- {:?}\n", place.0, entry);
            write!(f, "{}", indent(s));
        }
        write!(f, "\nresult <- {:#?}", self.result)
    }
}

pub enum Entry {
    IntEntry { value: i64 },
    BoolEntry { value: bool },
    CharEntry { value: char },
    RefEntry { el: Box<Entry> },
    Struct {
        name: String,
        field_entries: Vec<(String, Entry)>,
    },
    Enum {
        super_name: String,
        name: String,
        field_entries: Vec<(String, Entry)>,
        //note: if fields are not named, their order is important!
        //that is why no HashMap is used
    },
    Tuple {
        fields: Vec<Entry>,
    },
    Unit,
    UnknownEntry,
}

impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Entry::IntEntry { value } => write!(f, "{}", value),
            Entry::BoolEntry { value } => write!(f, "{}", value),
            Entry::CharEntry { value } => write!(f, "'{}' (0x{:x})", value, *value as i32),
            Entry::RefEntry { el } => write!(f, "ref({:#?})", el),
            Entry::Enum { super_name, name, field_entries } => {
                let named_fields = field_entries.len() > 0 && !field_entries[0].0.parse::<usize>().is_ok();
                let enum_name = format!("{}::{}", super_name, name);
                if named_fields {
                    let mut f1 = f.debug_struct(&enum_name);
                    for (fieldname, entry) in field_entries {
                        f1.field(fieldname, entry);
                    }
                    return f1.finish();
                } else {
                    let mut f1 = f.debug_tuple(&enum_name);
                    for (_, entry) in field_entries {
                        f1.field(entry);
                    }
                    return f1.finish();
                }
            },
            Entry::Struct { name, field_entries } => {
                let mut f1 = f.debug_struct(name);
                for (fieldname, entry) in field_entries {
                    f1.field(fieldname, entry);
                }
                f1.finish()

            },
            Entry::Tuple { fields } => {
                let mut f1 = f.debug_tuple("");
                for entry in fields {
                    f1.field(entry);
                }
                f1.finish()
            },
            Entry::Unit => write!(f, "()"),
            _ => write!(f, "?"),
        }
    }
}

//for printing multiline-entries, indents all but the first line
fn indent(s: String) -> String {
    let mut res = "".to_owned();
    let length = s.lines().count();
    let mut lines = s.lines();
    if length > 1 {
        res.push_str(lines.next().unwrap());
        res.push_str("\n");
        while let Some(l) = lines.next() {
            res.push_str("  ");
            res.push_str(l);
            res.push_str("\n");
        }
        res
    } else {
        s
    }
}


