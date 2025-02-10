// mod driver;
pub mod schema;
pub mod types;
mod util;

// macro_rules! plpgsql {
//     {
//         pub fn $name:ident ($($arg:ident : $ty:ty),* $(,)?) $(-> $ret:ty)? {
//             $($t:tt)*
//         }
//     } => {}
// }

// plpgsql! {
//     pub fn my_func(x: i32) -> i32 {
//         RETURN QUERY SELECT * FROM "employees" WHERE "age" >= "x";
//     }
// }

#[doc(hidden)]
pub mod __proc_macro_util {
    pub use std::borrow::Cow;

    pub use inventory::submit;

    pub use crate::schema::*;
}
