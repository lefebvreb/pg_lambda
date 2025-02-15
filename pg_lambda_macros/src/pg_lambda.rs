use proc_macro::TokenStream;
use proc_macro2::{Delimiter, TokenStream as TokenStream2, TokenTree};
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::token::{Brace, Paren};
use syn::{parenthesized, Ident, Result, ReturnType, Token, Type, Visibility};

use crate::util::ensure_let;

pub struct Input {
    pub ident: Ident,
    pub colon_token: Token![:],
    pub ty: Box<Type>,
}

impl Parse for Input {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            ident: input.parse()?,
            colon_token: input.parse()?,
            ty: input.parse()?,
        })
    }
}

pub struct Block {
    pub brace_token: Brace,
    pub plpgsql: String,
}

impl Parse for Block {
    fn parse(input: ParseStream) -> Result<Self> {
        let (brace_token, span) = input.step(|cursor| match cursor.token_tree() {
            Some((TokenTree::Group(g), cursor)) if g.delimiter() == Delimiter::Brace => {
                Ok(((Brace(g.delim_span()), g.span()), cursor))
            }
            _ => Err(cursor.error("expected braces")),
        })?;
        ensure_let!(
            Some(source) = span.source_text(),
            span,
            "failed to extract plpgsql code from this `pg_lambda` macro invocation",
        );
        let plpgsql = source[1..source.len() - 1].to_owned();
        Ok(Self {
            brace_token,
            plpgsql,
        })
    }
}

pub struct ItemPgLambda {
    pub vis: Visibility,
    pub fn_token: Token![fn],
    pub ident: Ident,
    pub paren_token: Paren,
    pub inputs: Punctuated<Input, Token![,]>,
    pub output: ReturnType,
    pub block: Block,
}

impl Parse for ItemPgLambda {
    fn parse(input: ParseStream) -> Result<Self> {
        let content;
        Ok(Self {
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

pub struct MacroInput {
    pub items: Vec<ItemPgLambda>,
}

impl Parse for MacroInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut items = vec![];
        while !input.is_empty() {
            items.push(input.parse()?);
        }
        Ok(Self { items })
    }
}

pub fn main(input: TokenStream) -> Result<TokenStream2> {
    let item = syn::parse::<MacroInput>(input)?;
    Ok(quote!())
}
