#![feature(box_patterns)]
#![feature(box_syntax)]

// #![allow(unused_imports)]
// #![deny(unused_must_use)]
// #![deny(unreachable_patterns)]
// #![deny(unused_mut)]
// #![deny(unused_variables)]
// #![deny(unused_imports)]
// #![deny(unused_doc_comments)]

extern crate config as config_crate;
#[macro_use]
extern crate log;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde;

include!(concat!(env!("OUT_DIR"), "/vir.rs"));

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
