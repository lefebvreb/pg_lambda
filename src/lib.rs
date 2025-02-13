// mod driver;
pub mod schema;
pub mod types;
mod util;

pub use pg_lambda_macros::pg_lambda;

#[doc(hidden)]
pub mod __proc_macro_util {
    pub use std::borrow::Cow;

    pub use inventory::submit;

    pub use crate::schema::*;

    pub const fn cow_slice<T: Clone>(slice: &'static [T]) -> Cow<'static, [T]> {
        Cow::Borrowed(slice)
    }
}
