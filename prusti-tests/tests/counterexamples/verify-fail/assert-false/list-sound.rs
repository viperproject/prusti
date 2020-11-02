#![feature(nll)]
#![feature(box_patterns)]
#![feature(box_syntax)]

use prusti_contracts::*;

/* 
    COUNTEREXAMPLE : 
        fn head(list):
            list <- List::Nil,
            ret <- None,
        
        fn tail(list):
            list <- List::Nil,
            ret <- None,
        
        fn length(list):
            list <- List::Nil,
            ret <- 0,

        fn last_value(list): 
            list <- List::Nil,
            ret <- None,

        fn empty_list(val): 
            val <- 42,

        fn singleton_list(val): 
            val <- 42,
            ret <- List::Const{
                    42,
                    next : box List::Nil,
            },
        fn prepend(val, list): 
            val <- 42,
            list <- List::Nil,
            ret <- List::Const {
                        val : 42,
                        next : box List::Nil
            },
        
        fn append(new_val, list){
            new_val <- 42,
            list <- List::Nil,
            ret <- List::Const {
                    val : new_val,
                    next : box List::Nil,
            },
        
        fn revert(list): 
            list <- List::Nil,
            ret <- List::Nil,
        
        (for all of them any other instantiation of the function 
            arguments would also be a valid counterexample)
*/


enum List {
    Nil,
    Const {
        val: i32,
        next: Box<List>
    },
}

fn head(list: List) -> Option<i32> {
    let ret = match list {
        List::Nil => None,
        List::Const { val, box next } => Some(val),
    };
    assert!(false);  //~ ERROR the asserted expression might not hold
    ret
}

fn tail(list: List) -> Option<List> {
    let ret = match list {
        List::Nil => None,
        List::Const { val, box next } => Some(next),
    };
    assert!(false);  //~ ERROR the asserted expression might not hold
    ret
}

fn length(list: List) -> u32 {
    let ret = match list {
        List::Nil => 0,
        List::Const { val, box next } => 1 + length(next),
    };
    assert!(false);  //~ ERROR the asserted expression might not hold
    ret
}

fn last_value(list: List) -> Option<i32> {
    let ret = match list {
        List::Nil => None,
        List::Const { val, box next } => match last_value(next) {
            None => Some(val),
            Some(last) => Some(last),
        }
    };
    assert!(false);  //~ ERROR the asserted expression might not hold
    ret
}

fn empty_list(val: i32) -> List {
    assert!(false);  //~ ERROR the asserted expression might not hold
    List::Nil
}

fn singleton_list(val: i32) -> List {
    let ret = List::Const {
        val,
        next: box List::Nil
    };
    assert!(false);  //~ ERROR the asserted expression might not hold
    ret
}

fn prepend(val: i32, list: List) -> List {
    let ret = List::Const {
        val,
        next: box list
    };
    assert!(false);  //~ ERROR the asserted expression might not hold
    ret
}

fn append(new_val: i32, list: List) -> List {
    let ret = match list {
        List::Nil => List::Const {
            val: new_val,
            next: box List::Nil
        },
        List::Const { val, box next } => List::Const {
            val: val,
            next: box append(new_val, next)
        },
    };
    assert!(false);  //~ ERROR the asserted expression might not hold
    ret
}

fn revert(list: List) -> List {
    let ret = match list {
        List::Nil => List::Nil,
        List::Const { val, box next } => append(val, revert(next))
    };
    assert!(false);  //~ ERROR the asserted expression might not hold
    ret
}

fn main() {}
