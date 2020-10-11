use std::collections::HashSet;
use std::hash::Hash;

pub(in super::super) trait LiftBindings<T, B> {
    fn lift_bindings(self) -> (Vec<T>, HashSet<B>);
}

impl<T, B, BI, I> LiftBindings<T, B> for I
    where
        BI: IntoIterator<Item=B>,
        I: Iterator<Item=(T, BI)>,
        B: Eq + Hash,
{
    fn lift_bindings(self) -> (Vec<T>, HashSet<B>) {
        let (objects, bindings): (_, Vec<_>) = self.unzip();
        let bindings = bindings.into_iter().flatten().collect();
        (objects, bindings)
    }
}
