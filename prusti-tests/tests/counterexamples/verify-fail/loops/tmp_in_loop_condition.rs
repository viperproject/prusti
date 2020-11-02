#![feature(box_syntax)]

use prusti_contracts::*;


/* COUNTEREXAMPLES : 
    (will always fail)
    fn test2():
        old(tmp) = box box 42,
        old(tmp_ref) <- &tmp,
        old(tmp_ref_mut) <- &mut tmp,
        tmp = box box 52,
        tmp_ref <- &tmp,
        tmp_ref_mut <- &mut tmp

*/

#[trusted]
fn random() -> u32 {
    unimplemented!()
}

fn test1() {
    let mut tmp = box box random();
    let tmp_ref = &tmp;
    let tmp_ref_mut = &mut tmp;

    while {
        let guard = random() < 55;

        let mut tmp = box box random();
        let tmp_ref = &tmp;
        let tmp_ref_mut = &mut tmp;

        guard
    } {
        let mut tmp = box box random();
        let tmp_ref = &tmp;
        let tmp_ref_mut = &mut tmp;
    }

    let mut tmp = box box random();
    let tmp_ref = &tmp;
    let tmp_ref_mut = &mut tmp;
    assert!(false); //~ ERROR
}

fn test2() {
    let mut tmp = box box random();
    let tmp_ref = &tmp;
    let tmp_ref_mut = &mut tmp;

    while {
        let guard = random() < 55;

        let mut tmp = box box random();
        let tmp_ref = &tmp;
        let tmp_ref_mut = &mut tmp;

        guard
    } {
        let mut tmp = box box random();
        let tmp_ref = &tmp;
        let tmp_ref_mut = &mut tmp;
        assert!(false); //~ ERROR
    }

    let mut tmp = box box random();
    let tmp_ref = &tmp;
    let tmp_ref_mut = &mut tmp;
}

fn main() {}
