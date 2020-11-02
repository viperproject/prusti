#![allow(dead_code, non_snake_case)]
use prusti_contracts::*;

/* COUNTEREXAMPLE : any instantiation of the 2 accounts
        fulfilling the precondition and an amount not equal to 0
        is a valid counterexample,
        error comes from + instead of -
        
        transfer(self, other, amount)
            old(self) <- Account{
                bal : 42,
            };
            old(other) <- Account{
                bal : 12,
            };
            amount <- 10
            self <- Account{
                bal = 32,
            };
            other <- {
                bal : 22,
            }
    */

struct Account {
    bal: u32,
}

impl Account {

    #[pure]
    fn balance(&self) -> u32 {
        self.bal
    }

    #[ensures(self.balance() == old(self.balance()) + amount)]
    fn deposit(&mut self, amount: u32) {
        self.bal = self.bal + amount;
    }

    #[requires(amount <= self.balance())]
    #[ensures(self.balance() == old(self.balance()) - amount)]
    fn withdraw(&mut self, amount: u32) {
        self.bal = self.bal - amount;
    }


    #[requires(amount <= self.balance())]
    #[ensures(self.balance() == old(self.balance()) + amount)] //~ ERROR postcondition might not hold
    fn transfer(&mut self, other: &mut Account, amount: u32) {
        self.withdraw(amount);
        other.deposit(amount);
    }
}

fn main() {}
