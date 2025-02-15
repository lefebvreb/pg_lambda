use md5::{Digest, Md5};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::token::Paren;
use syn::{parenthesized, Attribute, Ident, Item, LitStr, Result, ReturnType, Token, Type, Visibility};

use crate::util;

fn md5(input: &str) -> u128 {
    let mut hasher = Md5::new();
    hasher.update(input);
    u128::from_be_bytes(hasher.finalize().as_slice().try_into().unwrap())
}

struct Arg {
    ident: Ident,
    colon_token: Token![:],
    ty: Box<Type>,
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            ident: input.parse()?,
            colon_token: input.parse()?,
            ty: input.parse()?,
        })
    }
}

struct Lambda {
    attrs: Vec<Attribute>,
    vis: Visibility,
    fn_token: Token![fn],
    ident: Ident,
    paren_token: Paren,
    inputs: Punctuated<Arg, Token![,]>,
    output: ReturnType,
    block: LitStr,
}

impl Parse for Lambda {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(Self {
            attrs: input.call(Attribute::parse_outer)?,
            vis: input.parse()?,
            fn_token: input.parse()?,
            ident: input.parse()?,
            paren_token: parenthesized!(content in input),
            inputs: Punctuated::parse_terminated(&content)?,
            output: input.parse()?,
            block: input.parse()?,
        })
    }
}

struct Input {
    items: Vec<Lambda>,
}

impl Parse for Input {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut items = vec![];
        while !input.is_empty() {
            items.push(input.parse()?);
        }
        Ok(Self { items })
    }
}

pub fn main(input: TokenStream) -> Result<TokenStream2> {
    let input = syn::parse::<Input>(input)?;
    let this = util::crate_ident();
    let macro_util = quote!(::#this::__proc_macro_util);

    let mut tokens = Vec::new();
    
    for item in input.items {
        let plpgsql = item.block.value();
        let md5 = md5(&plpgsql);
        let name = format!("{}_{md5}", item.ident.to_string());
        let args = (0..item.inputs.len()).map(|i| format!("${}", i+1)).collect::<Vec<_>>().join(",");
        let statement = format!("SELECT \"{name}\"({args});");

        tokens.push(quote! {});
    }

    Ok(quote! {
        #(#tokens)*
    })
}
