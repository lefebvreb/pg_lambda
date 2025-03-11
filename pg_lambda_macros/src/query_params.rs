use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::token::Paren;
use syn::{Expr, Result, Token, Type, parenthesized};

use crate::util;

pub struct Param {
    ty: Type,
    _paren_token: Paren,
    expr: Expr,
}

impl Parse for Param {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(Self {
            ty: input.parse()?,
            _paren_token: parenthesized!(content in input),
            expr: content.parse()?,
        })
    }
}

struct Input {
    params: Punctuated<Param, Token![,]>,
}

impl Parse for Input {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            params: Punctuated::parse_terminated(input)?,
        })
    }
}

pub fn from_expr_type_pairs(macro_util: &TokenStream2, pairs: Vec<(Type, Expr)>) -> TokenStream2 {
    let count = pairs.len() as i16;
    let tokens = pairs.iter().map(|(ty, expr)| {
        quote! {
            #macro_util::write_value::<#ty, _>(#expr, dst)?;
        }
    });
    quote! {
        #macro_util::new_params_fn(|dst| {
            #macro_util::write_i16(#count, dst);
            #(#tokens)*
            Ok(())
        })
    }
}

pub fn main(input: TokenStream) -> Result<TokenStream2> {
    let input = syn::parse::<Input>(input)?;
    let macro_util = util::macro_util_path();
    let pairs = input
        .params
        .into_iter()
        .map(|param| (param.ty, param.expr))
        .collect();
    Ok(from_expr_type_pairs(&macro_util, pairs))
}
