use prusti_contracts::*;


pub struct Account {
    balance: i32,
}

fn get_balance(acc: Account) -> i32 {
    acc.balance
}

#[ensures(result)]
fn has_money(acc: Account) -> bool {
    get_balance(acc) > 0
}

fn main(){}