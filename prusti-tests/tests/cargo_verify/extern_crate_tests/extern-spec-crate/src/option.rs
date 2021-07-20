use prusti_contracts::*;

#[extern_spec]
impl<T> std::option::Option<T> {
    #[pure]
    #[ensures(matches!(*self, Some(_)) == result)]
    pub fn is_some(&self) -> bool;

    #[pure]
    #[ensures(self.is_some() == !result)]
    pub fn is_none(&self) -> bool;

    #[requires(self.is_some())]
    pub fn unwrap(self) -> T;

    pub fn unwrap_or(self, default: T) -> T;

    pub fn unwrap_or_else<F>(self, f: F) -> T
        where F: FnOnce() -> T;

    #[requires(self.is_some())]
    pub fn expect(self, msg: &str) -> T;

    pub fn as_ref(&self) -> Option<&T>;

    pub fn as_mut(&mut self) -> Option<&mut T>;
}