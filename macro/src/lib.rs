//! A macro for generating bounded integer structs and enums.
#![warn(
    clippy::pedantic,
    rust_2018_idioms,
    missing_docs,
    unused_qualifications
)]

use std::cmp;
use std::convert::TryInto;
use std::fmt::{self, Display, Formatter};
use std::ops::RangeInclusive;

use proc_macro2::{Ident, Literal, Span, TokenStream};
use quote::{quote, ToTokens, TokenStreamExt as _};
use syn::parse::{self, Parse, ParseStream};
use syn::{braced, parse_macro_input, token::Brace, Token};
use syn::{Attribute, Error, Expr, Path, Visibility};
use syn::{BinOp, ExprBinary, ExprRange, ExprUnary, RangeLimits, UnOp};
use syn::{ExprGroup, ExprParen};
use syn::{ExprLit, Lit};

use num_bigint::BigInt;

mod generate;

/// Generate a bounded integer type.
///
/// It takes in single struct or enum, with the content being any range expression, which can be
/// inclusive or not. The attributes and visibility (e.g. `pub`) of the type are forwarded directly
/// to the output type. It also implements:
/// * `Debug`, `Display`, `Binary`, `LowerExp`, `LowerHex`, `Octal`, `UpperExp` and `UpperHex`
/// * `Hash`
/// * `Clone` and `Copy`
/// * `PartialEq` and `Eq`
/// * `PartialOrd` and `Ord`
/// * If the `serde` feature is enabled, `Serialize` and `Deserialize`
///
/// # Examples
///
/// With a struct:
/// ```
/// # mod force_item_scope {
/// # use bounded_integer_macro::bounded_integer;
/// # #[cfg(not(feature = "serde"))]
/// bounded_integer! {
///     pub struct S { -3..2 }
/// }
/// # }
/// ```
/// The generated item should look like this (i8 is chosen as it is the smallest repr):
/// ```
/// #[derive(Debug, Hash, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// pub struct S(i8);
/// ```
/// And the methods will ensure that `-3 <= S.0 < 2`.
///
/// With an enum:
/// ```
/// # mod force_item_scope {
/// # use bounded_integer_macro::bounded_integer;
/// # #[cfg(not(feature = "serde"))]
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
/// # mod force_item_scope {
/// # use bounded_integer_macro::bounded_integer;
/// # #[cfg(not(feature = "serde"))]
/// bounded_integer! {
///     #[repr(u16)]
///     pub struct S { 2..5 }
/// }
/// # }
/// ```
/// The generated item should look like this:
/// ```
/// #[derive(Debug, Hash, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// pub struct S(u16);
/// ```
///
/// # Custom path to bounded integer
///
/// `bounded-integer` will assume that it is located at `::bounded_integer` by default. You can
/// override this by adding a `bounded_integer` attribute to your item. For example if
/// `bounded_integer` is instead located at `path::to::bounded_integer`:
///
/// ```ignore
/// # mod force_item_scope {
/// # use bounded_integer_macro::bounded_integer;
/// bounded_integer! {
///     #[repr(i8)]
///     #[bounded_integer = path::to::bounded_integer]
///     pub struct S { 5..7 }
/// }
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
#[proc_macro]
pub fn bounded_integer(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let item = parse_macro_input!(input as BoundedInteger);

    let mut result = TokenStream::new();
    generate::generate(&item, &mut result);
    result.into()
}

macro_rules! signed {
    (unsigned) => {
        false
    };
    (signed) => {
        true
    };
}

struct BoundedInteger {
    attrs: Vec<Attribute>,
    #[cfg(feature = "serde")]
    serde: TokenStream,
    repr: Repr,
    vis: Visibility,
    kind: Kind,
    ident: Ident,
    brace_token: Brace,
    range: RangeInclusive<BigInt>,
}

impl Parse for BoundedInteger {
    fn parse(input: ParseStream<'_>) -> parse::Result<Self> {
        let mut attrs = input.call(Attribute::parse_outer)?;

        let repr_pos = attrs.iter().position(|attr| attr.path.is_ident("repr"));
        let repr = repr_pos
            .map(|pos| attrs.remove(pos).parse_args::<Repr>())
            .transpose()?;

        let crate_location_pos = attrs
            .iter()
            .position(|attr| attr.path.is_ident("bounded_integer"));
        #[cfg_attr(not(feature = "serde"), allow(unused_variables))]
        let crate_location = crate_location_pos
            .map(|crate_location_pos| -> parse::Result<_> {
                struct CrateLocation(Path);
                impl Parse for CrateLocation {
                    fn parse(input: ParseStream<'_>) -> parse::Result<Self> {
                        input.parse::<Token![=]>()?;
                        Ok(Self(input.parse::<Path>()?))
                    }
                }

                let location: CrateLocation = syn::parse2(attrs.remove(crate_location_pos).tokens)?;
                Ok(location.0.into_token_stream())
            })
            .transpose()?
            .unwrap_or_else(|| quote!(::bounded_integer));
        #[cfg(feature = "serde")]
        let serde = quote!(#crate_location::__serde);

        let vis: Visibility = input.parse()?;

        let kind: Kind = input.parse()?;

        let ident: Ident = input.parse()?;

        let range_tokens;
        let brace_token = braced!(range_tokens in input);
        let range: ExprRange = range_tokens.parse()?;

        let (from_expr, to_expr) = match range.from.as_deref().zip(range.to.as_deref()) {
            Some(t) => t,
            None => return Err(Error::new_spanned(range, "Range must be closed")),
        };
        let from = eval_expr(from_expr)?;
        let to = eval_expr(to_expr)?;
        let to = if let RangeLimits::HalfOpen(_) = range.limits {
            to - 1
        } else {
            to
        };
        if from >= to {
            return Err(Error::new_spanned(
                range,
                "The start of the range must be before the end",
            ));
        }

        let repr = match repr {
            Some(explicit_repr) => {
                if !explicit_repr.signed && from.sign() == num_bigint::Sign::Minus {
                    return Err(Error::new_spanned(
                        from_expr,
                        "An unsigned integer cannot hold a negative value",
                    ));
                }

                if explicit_repr.minimum().map_or(false, |min| from < min) {
                    return Err(Error::new_spanned(
                        from_expr,
                        format_args!(
                            "Bound {} is below the minimum value for the underlying type",
                            from
                        ),
                    ));
                }
                if explicit_repr.maximum().map_or(false, |max| to > max) {
                    return Err(Error::new_spanned(
                        to_expr,
                        format_args!(
                            "Bound {} is above the maximum value for the underlying type",
                            to
                        ),
                    ));
                }

                explicit_repr
            }
            None => Repr::smallest_repr(&from, &to).ok_or_else(|| {
                Error::new_spanned(range, "Range is too wide to fit in any integer primitive")
            })?,
        };

        Ok(Self {
            attrs,
            #[cfg(feature = "serde")]
            serde,
            repr,
            vis,
            kind,
            ident,
            brace_token,
            range: from..=to,
        })
    }
}

enum Kind {
    Struct(Token![struct]),
    Enum(Token![enum]),
}

impl Parse for Kind {
    fn parse(input: ParseStream<'_>) -> parse::Result<Self> {
        Ok(if input.peek(Token![struct]) {
            Self::Struct(input.parse()?)
        } else {
            Self::Enum(input.parse()?)
        })
    }
}

struct Repr {
    signed: bool,
    size: ReprSize,
    name: Ident,
}

impl Repr {
    fn new(signed: bool, size: ReprSize) -> Self {
        Self {
            signed,
            size,
            name: Ident::new(
                &format!("{}{}", if signed { 'i' } else { 'u' }, size),
                Span::call_site(),
            ),
        }
    }

    fn smallest_repr(min: &BigInt, max: &BigInt) -> Option<Self> {
        Some(if min.sign() == num_bigint::Sign::Minus {
            Self::new(
                true,
                ReprSize::Fixed(cmp::max(
                    ReprSizeFixed::from_bits(min.bits() + 1)?,
                    ReprSizeFixed::from_bits(max.bits() + 1)?,
                )),
            )
        } else {
            Self::new(
                false,
                ReprSize::Fixed(ReprSizeFixed::from_bits(max.bits())?),
            )
        })
    }

    fn minimum(&self) -> Option<BigInt> {
        Some(match (self.signed, self.size) {
            (false, ReprSize::Fixed(ReprSizeFixed::Fixed8)) => BigInt::from(u8::MIN),
            (false, ReprSize::Fixed(ReprSizeFixed::Fixed16)) => BigInt::from(u16::MIN),
            (false, ReprSize::Fixed(ReprSizeFixed::Fixed32)) => BigInt::from(u32::MIN),
            (false, ReprSize::Fixed(ReprSizeFixed::Fixed64)) => BigInt::from(u64::MIN),
            (false, ReprSize::Fixed(ReprSizeFixed::Fixed128)) => BigInt::from(u128::MIN),
            (true, ReprSize::Fixed(ReprSizeFixed::Fixed8)) => BigInt::from(i8::MIN),
            (true, ReprSize::Fixed(ReprSizeFixed::Fixed16)) => BigInt::from(i16::MIN),
            (true, ReprSize::Fixed(ReprSizeFixed::Fixed32)) => BigInt::from(i32::MIN),
            (true, ReprSize::Fixed(ReprSizeFixed::Fixed64)) => BigInt::from(i64::MIN),
            (true, ReprSize::Fixed(ReprSizeFixed::Fixed128)) => BigInt::from(i128::MIN),
            (_, ReprSize::Pointer) => return None,
        })
    }
    fn maximum(&self) -> Option<BigInt> {
        Some(match (self.signed, self.size) {
            (false, ReprSize::Fixed(ReprSizeFixed::Fixed8)) => BigInt::from(u8::MAX),
            (false, ReprSize::Fixed(ReprSizeFixed::Fixed16)) => BigInt::from(u16::MAX),
            (false, ReprSize::Fixed(ReprSizeFixed::Fixed32)) => BigInt::from(u32::MAX),
            (false, ReprSize::Fixed(ReprSizeFixed::Fixed64)) => BigInt::from(u64::MAX),
            (false, ReprSize::Fixed(ReprSizeFixed::Fixed128)) => BigInt::from(u128::MAX),
            (true, ReprSize::Fixed(ReprSizeFixed::Fixed8)) => BigInt::from(i8::MAX),
            (true, ReprSize::Fixed(ReprSizeFixed::Fixed16)) => BigInt::from(i16::MAX),
            (true, ReprSize::Fixed(ReprSizeFixed::Fixed32)) => BigInt::from(i32::MAX),
            (true, ReprSize::Fixed(ReprSizeFixed::Fixed64)) => BigInt::from(i64::MAX),
            (true, ReprSize::Fixed(ReprSizeFixed::Fixed128)) => BigInt::from(i128::MAX),
            (_, ReprSize::Pointer) => return None,
        })
    }

    fn number_literal(&self, value: &BigInt) -> Literal {
        macro_rules! match_repr {
            ($($sign:ident $size:ident $(($fixed:ident))? => $f:ident,)*) => {
                match (self.signed, self.size) {
                    $((signed!($sign), ReprSize::$size $((ReprSizeFixed::$fixed))?) => {
                        Literal::$f(value.try_into().unwrap())
                    })*
                }
            }
        }

        match_repr! {
            unsigned Fixed(Fixed8) => u8_suffixed,
            unsigned Fixed(Fixed16) => u16_suffixed,
            unsigned Fixed(Fixed32) => u32_suffixed,
            unsigned Fixed(Fixed64) => u64_suffixed,
            unsigned Fixed(Fixed128) => u128_suffixed,
            unsigned Pointer => usize_suffixed,
            signed Fixed(Fixed8) => i8_suffixed,
            signed Fixed(Fixed16) => i16_suffixed,
            signed Fixed(Fixed32) => i32_suffixed,
            signed Fixed(Fixed64) => i64_suffixed,
            signed Fixed(Fixed128) => i128_suffixed,
            signed Pointer => isize_suffixed,
        }
    }
}

impl Parse for Repr {
    fn parse(input: ParseStream<'_>) -> parse::Result<Self> {
        let name = input.parse::<Ident>()?;
        let span = name.span();
        let s = name.to_string();

        let (size, signed) = if let Some(size) = s.strip_prefix("i") {
            (size, true)
        } else if let Some(size) = s.strip_prefix("u") {
            (size, false)
        } else {
            return Err(Error::new(span, "Repr must a primitive integer type"));
        };

        let size = match size {
            "8" => ReprSize::Fixed(ReprSizeFixed::Fixed8),
            "16" => ReprSize::Fixed(ReprSizeFixed::Fixed16),
            "32" => ReprSize::Fixed(ReprSizeFixed::Fixed32),
            "64" => ReprSize::Fixed(ReprSizeFixed::Fixed64),
            "128" => ReprSize::Fixed(ReprSizeFixed::Fixed128),
            "size" => ReprSize::Pointer,
            unknown => {
                return Err(Error::new(
                    span,
                    format_args!(
                        "Unknown integer size {}, must be one of 8, 16, 32, 64, 128 or size",
                        unknown
                    ),
                ));
            }
        };

        Ok(Self { signed, size, name })
    }
}

impl ToTokens for Repr {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        tokens.append(self.name.clone());
    }
}

#[derive(Clone, Copy)]
enum ReprSize {
    Fixed(ReprSizeFixed),
    /// `usize`/`isize`
    Pointer,
}

impl Display for ReprSize {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Fixed(fixed) => fixed.fmt(f),
            Self::Pointer => f.write_str("size"),
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum ReprSizeFixed {
    Fixed8,
    Fixed16,
    Fixed32,
    Fixed64,
    Fixed128,
}

impl ReprSizeFixed {
    fn from_bits(bits: u64) -> Option<Self> {
        Some(match bits {
            0..=8 => Self::Fixed8,
            9..=16 => Self::Fixed16,
            17..=32 => Self::Fixed32,
            33..=64 => Self::Fixed64,
            65..=128 => Self::Fixed128,
            129..=u64::MAX => return None,
        })
    }
}

impl Display for ReprSizeFixed {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Fixed8 => "8",
            Self::Fixed16 => "16",
            Self::Fixed32 => "32",
            Self::Fixed64 => "64",
            Self::Fixed128 => "128",
        })
    }
}

fn eval_expr(expr: &Expr) -> syn::Result<BigInt> {
    Ok(match expr {
        Expr::Lit(ExprLit { lit, .. }) => match lit {
            Lit::Int(int) => int.base10_parse()?,
            _ => {
                return Err(Error::new_spanned(lit, "literal must be integer"));
            }
        },
        Expr::Unary(ExprUnary { op, expr, .. }) => {
            let expr = eval_expr(&expr)?;
            match op {
                UnOp::Not(_) => !expr,
                UnOp::Neg(_) => -expr,
                _ => {
                    return Err(Error::new_spanned(op, "unary operator must be ! or -"));
                }
            }
        }
        Expr::Binary(ExprBinary {
            left, op, right, ..
        }) => {
            let left = eval_expr(&left)?;
            let right = eval_expr(&right)?;
            match op {
                BinOp::Add(_) => left + right,
                BinOp::Sub(_) => left - right,
                BinOp::Mul(_) => left * right,
                BinOp::Div(_) => left
                    .checked_div(&right)
                    .ok_or_else(|| Error::new_spanned(op, "Attempted to divide by zero"))?,
                BinOp::Rem(_) => left % right,
                BinOp::BitXor(_) => left ^ right,
                BinOp::BitAnd(_) => left & right,
                BinOp::BitOr(_) => left | right,
                _ => {
                    return Err(Error::new_spanned(
                        op,
                        "operator not supported in this context",
                    ));
                }
            }
        }
        Expr::Group(ExprGroup { expr, .. }) | Expr::Paren(ExprParen { expr, .. }) => {
            eval_expr(expr)?
        }
        _ => return Err(Error::new_spanned(expr, "expected simple expression")),
    })
}
