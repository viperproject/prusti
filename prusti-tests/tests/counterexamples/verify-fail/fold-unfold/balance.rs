use prusti_contracts::*;


/* COUNTEREXAMPLES : 
Well this DOES verify, so it seems to be in the wrong place? 
Overflow checks make a lot more fail than just the annotated function
 */

struct Account {
    bal: u32,
}

impl Account {

    #[pure]
    fn balance(&self) -> u32 {
        self.bal
    }

    #[ensures(self.balance() == old(self.bal) + amount)]
    fn deposit(&mut self, amount: u32) {
        self.bal = self.bal + amount;
    }


    #[ensures(self.bal == old(self.bal) - amount)]
    fn withdraw(&mut self, amount: u32) { //~ ERROR implicit type invariants might not hold at the end of the method.
        self.bal = self.bal - amount;
    }

    #[ensures(self.bal == old(self.bal) - amount)]
    #[ensures(self.bal + other.bal == old(self.bal + other.bal))]
    fn transfer(&mut self, other: &mut Account, amount: u32) {
        self.withdraw(amount);
        other.deposit(amount);
    }
}

fn main() {}
