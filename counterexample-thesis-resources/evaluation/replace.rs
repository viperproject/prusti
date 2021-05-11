use prusti_contracts::*;

fn main(){}

fn replace(x: &mut char, acc: bool) {
    match x {
        '$' => {
            if acc {
                *x = ' ';
            } else {
               panic!("no access"); 
            }
        },
        _ => {}
    }
}