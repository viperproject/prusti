use prusti_contracts::*;


pub struct Account {
    balance: i32,
}

#[requires(amount > 0 && x.balance > amount && y.balance >= 0)]
#[ensures(old(y.balance) > result.1.balance)]
pub fn transfer(
    mut x: Account, 
    mut y: Account, 
    amount: i32
) -> (Account, Account) {
    if x.balance >= amount {
        let temp = y.balance;
        x.balance -= amount;
        y.balance += amount;
    }
    (x, y)
}

fn main(){}