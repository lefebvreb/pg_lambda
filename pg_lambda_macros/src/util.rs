use proc_macro::TokenStream;
use proc_macro_crate::FoundCrate;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::Result;

const MAIN_CRATE_NAME: &str = "pg_lambda";

pub fn macro_util_path() -> TokenStream2 {
    match proc_macro_crate::crate_name(MAIN_CRATE_NAME) {
        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{name}");
            quote!(::#ident::__proc_macro_util)
        }
        Ok(FoundCrate::Itself) => quote!(crate::__proc_macro_util),
        _ => {
            let ident = format_ident!("{MAIN_CRATE_NAME}");
            quote!(::#ident::__proc_macro_util)
        }
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
