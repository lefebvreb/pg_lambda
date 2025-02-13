use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use proc_macro_crate::FoundCrate;
use quote::format_ident;
use syn::{Ident, Result};

const CRATE_NAME: &str = "pg_lambda";

pub fn crate_ident() -> Ident {
    match proc_macro_crate::crate_name(CRATE_NAME) {
        Ok(FoundCrate::Name(name)) => format_ident!("{name}"),
        _ => format_ident!("{CRATE_NAME}"),
    }
}

pub fn syn_try(f: impl FnOnce() -> Result<TokenStream2>) -> TokenStream {
    f().unwrap_or_else(|err| err.to_compile_error()).into()
}

macro_rules! bail {
    ($span:expr, $($arg:tt)+) => {
        return Err(::syn::Error::new($span, ::std::format!($($arg)+)))
    };
}

macro_rules! ensure {
    ($cond:expr, $span:expr, $($arg:tt)+) => {
        if !$cond {
            crate::util::bail!($span, $($arg)+);
        }
    };
}

macro_rules! ensure_let {
    ($pat:pat = $expr:expr, $span:expr, $($arg:tt)+) => {
        let $pat = $expr else {
            crate::util::bail!($span, $($arg)+);
        };
    };
}

pub(crate) use {bail, ensure, ensure_let};
