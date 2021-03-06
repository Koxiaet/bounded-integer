//! This crate provides two types of bounded integer.
//!
//! # Macro-generated bounded integers
//!
//! The [`bounded_integer!`] macro allows you to define your own bounded integer type, given a
//! specific range it inhabits. For example:
//!
//! ```rust
#![cfg_attr(not(feature = "macro"), doc = "# #[cfg(any())] {")]
#![cfg_attr(feature = "step_trait", doc = "# #![feature(step_trait)]")]
//! # use bounded_integer::bounded_integer;
//! bounded_integer! {
//!     struct MyInteger { 0..8 }
//! }
//! let num = MyInteger::new(5).unwrap();
//! assert_eq!(num, 5);
#![cfg_attr(not(feature = "macro"), doc = "# }")]
//! ```
//!
//! This macro supports both `struct`s and `enum`s. See the [`examples`] module for the
//! documentation of generated types.
//!
//! # Const generics-based bounded integers
//!
//! You can also create ad-hoc bounded integers via types in this library that use const generics,
//! for example:
//!
//! ```rust
#![cfg_attr(feature = "step_trait", doc = "# #![feature(step_trait)]")]
#![cfg_attr(not(feature = "types"), doc = "# #[cfg(any())] {")]
//! # use bounded_integer::BoundedU8;
//! let num = <BoundedU8<0, 7>>::new(5).unwrap();
//! assert_eq!(num, 5);
#![cfg_attr(not(feature = "types"), doc = "# }")]
//! ```
//!
//! These integers are shorter to use as they don't require a type declaration or explicit name,
//! and they interoperate better with other integers that have different ranges. However due to the
//! limits of const generics, they do not implement some traits like `Default`.
//!
//! # `no_std`
//!
//! All the integers in this crate depend only on libcore and so work in `#![no_std]` environments.
//!
//! # Crate Features
//!
//! By default, no crate features are enabled.
//! - `macro`: Enable the [`bounded_integer!`] macro.
//! - `types`: Enable the bounded integer types that use const generics.
//! - `serde`: Implement `Serialize` and `Deserialize` for the bounded integers, making sure all
//! values will never be out of bounds.
//! - `step_trait`: Implement the [`Step`] trait which allows the bounded integers to be easily used
//! in ranges. This will require you to use nightly and place `#![feature(step_trait)]` in your
//! crate root if you use the macro.
//!
//! [`bounded_integer!`]: https://docs.rs/bounded-integer/*/bounded_integer/macro.bounded_integer.html
//! [`examples`]: https://docs.rs/bounded-integer/*/bounded_integer/examples/
//! [`Step`]: https://doc.rust-lang.org/nightly/core/iter/trait.Step.html
#![cfg_attr(feature = "step_trait", feature(step_trait))]
#![cfg_attr(doc_cfg, feature(doc_cfg))]
#![no_std]

#[cfg(feature = "types")]
mod types;
#[cfg(feature = "types")]
pub use types::*;

#[doc(hidden)]
#[cfg(feature = "macro")]
pub mod __private {
    #[cfg(feature = "serde")]
    pub use ::serde;

    #[cfg(all(not(feature = "serde"), not(feature = "step_trait")))]
    pub use bounded_integer_macro::not_serde_not_step_trait as proc_macro;
    #[cfg(all(not(feature = "serde"), feature = "step_trait"))]
    pub use bounded_integer_macro::not_serde_step_trait as proc_macro;
    #[cfg(all(feature = "serde", not(feature = "step_trait")))]
    pub use bounded_integer_macro::serde_not_step_trait as proc_macro;
    #[cfg(all(feature = "serde", feature = "step_trait"))]
    pub use bounded_integer_macro::serde_step_trait as proc_macro;
}

#[cfg(feature = "__examples")]
pub mod examples;

/// Generate a bounded integer type.
///
/// It takes in single struct or enum, with the content being a bounded range expression, whose
/// upper bound can be inclusive (`x..=y`) or exclusive (`x..y`). The attributes and visibility
/// (e.g. `pub`) of the type are forwarded directly to the output type.
///
/// See the [`examples`] module for examples of what this macro generates.
///
/// # Examples
///
/// With a struct:
/// ```
#[cfg_attr(feature = "step_trait", doc = "# #![feature(step_trait)]")]
/// # mod force_item_scope {
/// # use bounded_integer::bounded_integer;
/// bounded_integer! {
///     pub struct S { -3..2 }
/// }
/// # }
/// ```
/// The generated item should look like this (i8 is chosen as it is the smallest repr):
/// ```
/// #[derive(Debug, Hash, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// #[repr(transparent)]
/// pub struct S(i8);
/// ```
/// And the methods will ensure that `-3 <= S.0 < 2`.
///
/// With an enum:
/// ```
#[cfg_attr(feature = "step_trait", doc = "# #![feature(step_trait)]")]
/// # mod force_item_scope {
/// # use bounded_integer::bounded_integer;
/// bounded_integer! {
///     pub enum S { 5..=7 }
/// }
/// # }
/// ```
/// The generated item should look like this (u8 is chosen as it is the smallest repr):
/// ```
/// #[derive(Debug, Hash, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// #[repr(u8)]
/// pub enum S {
///     P5 = 5, P6, P7
/// }
/// ```
///
/// # Custom repr
///
/// The item can have a `repr` attribute to specify how it will be represented in memory, which can
/// be a `u*` or `i*` type. In this example we override the `repr` to be a `u16`, when it would
/// have normally been a `u8`.
///
/// ```
#[cfg_attr(feature = "step_trait", doc = "# #![feature(step_trait)]")]
/// # mod force_item_scope {
/// # use bounded_integer::bounded_integer;
/// bounded_integer! {
///     #[repr(u16)]
///     pub struct S { 2..5 }
/// }
/// # }
/// ```
/// The generated item should look like this:
/// ```
/// #[derive(Debug, Hash, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// #[repr(transparent)]
/// pub struct S(u16);
/// ```
///
/// # Limitations
///
/// - Both bounds of ranges must be closed and a simple const expression involving only literals and
/// the following operators:
///     - Negation (`-x`)
///     - Addition (`x+y`), subtraction (`x-y`), multiplication (`x*y`), division (`x/y`) and
///     remainder (`x%y`).
///     - Bitwise not (`!x`), XOR (`x^y`), AND (`x&y`) and OR (`x|y`).
#[cfg(feature = "macro")]
#[cfg_attr(doc_cfg, doc(cfg(feature = "macro")))]
#[macro_export]
macro_rules! bounded_integer {
    ($($tt:tt)*) => {
        $crate::__private::proc_macro!([$crate] $($tt)*);
    };
}
