extern crate proc_macro;

#[cfg(not(feature = "prusti"))]
mod private {
    /// A macro for writing a precondition on a function.
    pub use prusti_contracts_impl::requires;

    /// A macro for writing a postcondition on a function.
    pub use prusti_contracts_impl::ensures;

    /// A macro for writing a pledge on a function.
    pub use prusti_contracts_impl::pledge;

    /// A macro for marking a function as pure.
    pub use prusti_contracts_impl::pure;

    /// A macro for marking a function as trusted.
    pub use prusti_contracts_impl::trusted;

    /// A macro for writing a loop body invariant.
    pub use prusti_contracts_impl::body_invariant;

    /// A macro for defining a closure with a specification.
    pub use prusti_contracts_impl::closure;

    /// A macro for impl blocks that refine trait specifications.
    pub use prusti_contracts_impl::refine_trait_spec;

    /// A macro for specifying external functions.
    pub use prusti_contracts_impl::extern_spec;
}

#[cfg(feature = "prusti")]
mod private {
    /// A macro for writing a precondition on a function.
    pub use prusti_contracts_internal::requires;

    /// A macro for writing a postcondition on a function.
    pub use prusti_contracts_internal::ensures;

    /// A macro for writing a pledge on a function.
    pub use prusti_contracts_internal::pledge;

    /// A macro for marking a function as pure.
    pub use prusti_contracts_internal::pure;

    /// A macro for marking a function as trusted.
    pub use prusti_contracts_internal::trusted;

    /// A macro for writing a loop body invariant.
    pub use prusti_contracts_internal::body_invariant;

    /// A macro for defining a closure with a specification.
    pub use prusti_contracts_internal::closure;

    /// A macro for impl blocks that refine trait specifications.
    pub use prusti_contracts_internal::refine_trait_spec;

    /// A macro for specifying external functions.
    pub use prusti_contracts_internal::extern_spec;
}


/// This function is used to evaluate an expression in the context just
/// before the borrows expires.
pub fn before_expiry<R, T>(_: R, arg: T) -> T {
    arg
}

/// This function is used to evaluate an expression in the context just after a reference is
/// unblocked.
pub fn after_unblocked<R, T>(_: R, arg: T) -> T {
    arg
}

/// This function is used to evaluate an expression in the “old”
/// context, that is at the beginning of the method call.
pub fn old<T>(arg: T) -> T {
    arg
}

pub use private::*;
