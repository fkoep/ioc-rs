#![feature(box_syntax)]
#![feature(unsize)]

#[macro_use]
extern crate derive_more;
#[macro_use]
extern crate downcast;
#[macro_use]
extern crate quick_error;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate variadic_generics;

mod resolve;
mod reflect;
mod middleware;
mod composition;

pub use resolve::*;
pub use reflect::*;
pub use middleware::*;
pub use composition::*;

pub use TypedInstanceHandle as I;
pub fn root() -> Composition { Composition::root() }
pub fn redirect_rules() -> RedirectRules { RedirectRules::default() }
