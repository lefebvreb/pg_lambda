#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod connection;
pub mod lambda;
pub mod migrations;
pub mod schema;
pub mod types;
mod util;

pub mod prelude {
    pub use crate::lambda::pg_lambda;
    #[doc(inline)]
    pub use crate::schema::Table;
}

#[doc(hidden)]
pub mod __proc_macro_util;
