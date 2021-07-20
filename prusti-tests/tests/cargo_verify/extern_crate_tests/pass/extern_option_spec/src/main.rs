use prusti_contracts::*;
use extern_spec_crate;

prusti_use!(extern_spec_crate::std::option::Option<T>);
fn main() {
    let mut x = Some(3);
    assert!(x.is_some());
    x = None;
    assert!(x.is_none());
}
