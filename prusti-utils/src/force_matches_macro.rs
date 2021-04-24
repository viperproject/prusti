#[macro_export]
macro_rules! force_matches {
    ($ex:expr, $patt:pat => $ret:expr) => {
        if let $patt = $ex {
            $ret
        } else {
            unreachable!("force_matches: expr {} doesn't match pattern {}", stringify!($ex), stringify!($patt))
        }
    }
}
