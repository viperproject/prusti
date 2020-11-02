/* COUNTEREXAMPLES : 
    fn test_panic(x):
        x <- true
        (fails always)
    fn test_assert(x):
        x <- true
        (fails always)
    fn test_assert_msg(x):
        x <- true
        (fails always)
    test_debug_assert(x):
        x <- true
        (fails always)
    test_debug_assert_msg(x):
        x <- true
        (fails always)
    test_unreachable(x):
        x <- true
        (fails always)
    fn test_unimplemented(x):
        x <- true
        (fails always)
    macro inner_panic():
        empty
    macro inner_panic_msg():
        empty
    macro inner_assert():
        empty
    macro inner_assert_msg():
        empty
    macro inner_debug_assert():
        empty
    macro inner_debug_assert_msg():
        empty       
    macro inner_unreachable():
        empty
    macro inner_unimplemented()
        empty
*/


fn test_panic(x: bool) {
    panic!();  //~ ERROR panic!(..) statement might be reachable
}

fn test_assert(x: bool) {
    assert!(false);  //~ ERROR the asserted expression might not hold
}

fn test_assert_msg(x: bool) {
    assert!(false, "msg");  //~ ERROR the asserted expression might not hold
}

fn test_debug_assert(x: bool) {
    debug_assert!(false);  //~ ERROR the asserted expression might not hold
}

fn test_debug_assert_msg(x: bool) {
    debug_assert!(false, "msg");  //~ ERROR the asserted expression might not hold
}

fn test_unreachable(x: bool) {
    unreachable!();  //~ ERROR unreachable!(..) statement might be reachable
}

fn test_unimplemented(x: bool) {
    unimplemented!();  //~ ERROR unimplemented!(..) statement might be reachable
}

macro_rules! inner_panic {
    () => {
        {
            panic!();  //~ ERROR panic!(..) statement might be reachable
        }
    };
}

fn test_inner_panic(x: bool) {
    inner_panic!();
}

macro_rules! inner_panic_msg {
    () => {
        {
            panic!("msg");  //~ ERROR panic!(..) statement might be reachable
        }
    };
}

fn test_inner_panic_msg(x: bool) {
    inner_panic_msg!();
}

macro_rules! inner_assert {
    () => {
        {
            assert!(false);  //~ ERROR the asserted expression might not hold
        }
    };
}

fn test_inner_assert(x: bool) {
    inner_assert!();
}

macro_rules! inner_assert_msg {
    () => {
        {
            assert!(false, "msg");  //~ ERROR the asserted expression might not hold
        }
    };
}

fn test_inner_assert_msg(x: bool) {
    inner_assert_msg!();
}

macro_rules! inner_debug_assert {
    () => {
        {
            debug_assert!(false);  //~ ERROR the asserted expression might not hold
        }
    };
}

fn test_inner_debug_assert(x: bool) {
    inner_debug_assert!();
}

macro_rules! inner_debug_assert_msg {
    () => {
        {
            debug_assert!(false, "msg");  //~ ERROR the asserted expression might not hold
        }
    };
}

fn test_inner_debug_assert_msg(x: bool) {
    inner_debug_assert_msg!();
}

macro_rules! inner_unreachable {
    () => {
        {
            unreachable!();  //~ ERROR unreachable!(..) statement might be reachable
        }
    };
}

fn test_inner_unreachable(x: bool) {
    inner_unreachable!();
}

macro_rules! inner_unimplemented {
    () => {
        {
            unimplemented!();  //~ ERROR unimplemented!(..) statement might be reachable
        }
    };
}

fn test_inner_unimplemented(x: bool) {
    inner_unimplemented!();
}

fn main() {}
