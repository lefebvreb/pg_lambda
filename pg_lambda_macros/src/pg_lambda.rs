use md5::{Digest, Md5};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::token::{Brace, Paren};
use syn::{braced, parenthesized, Attribute, Ident, LitStr, Result, Token, Type, TypePath, Visibility};

use crate::util;

fn md5(input: &str) -> u128 {
    let mut hasher = Md5::new();
    hasher.update(input);
    u128::from_be_bytes(hasher.finalize().as_slice().try_into().unwrap())
}

struct LambdaArg {
    ident: Ident,
    colon_token: Token![:],
    ty: Box<Type>,
}

impl Parse for LambdaArg {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Self {
            ident: input.parse()?,
            colon_token: input.parse()?,
            ty: input.parse()?,
        })
    }
}

enum ReturnType {
    Default,
    Type(Token![->], Box<Type>),
    Table {
        rarrow: Token![->],
        _table_ident: Ident,
        _brace_token: Brace,
        columns: Punctuated<LambdaArg, Token![,]>,
    },
}

impl Parse for ReturnType {
    fn parse(input: ParseStream) -> Result<Self> {
        if !input.peek(Token![->]) {
            return Ok(Self::Default)
        }
        
        let rarrow = input.parse()?;
        let ty = input.parse::<Box<Type>>()?;

        Ok(match ty.as_ref() {
            Type::Path(TypePath { path, .. }) if path.is_ident("Table") => {
                let content;
                Self::Table { 
                    rarrow, 
                    _table_ident: path.get_ident().unwrap().clone(), 
                    _brace_token: braced!(content in input), 
                    columns: Punctuated::parse_terminated(&content)?,
                }
            },
            _ => Self::Type(rarrow, ty),
        })
    }
}

struct Lambda {
    attrs: Vec<Attribute>,
    vis: Visibility,
    fn_token: Token![fn],
    ident: Ident,
    _paren_token: Paren,
    inputs: Punctuated<LambdaArg, Token![,]>,
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
            _paren_token: parenthesized!(content in input),
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
    
    for Lambda { attrs, vis, fn_token, ident, inputs, output, block, .. } in input.items {
        let (name, create_statement, select_statement) = {
            let body = block.value();
            let md5 = md5(&body); // take inputs/output in account when computing md5 hash
            let name = format!("{ident}_{md5}");

            let select_params = (1..=inputs.len()).map(|i| format!("${i}")).collect::<Vec<_>>().join(",");
            let select_statement = format!("SELECT \"{name}\"({select_params});");

            let args = inputs
                .iter()
                .map(|LambdaArg { ident, ty, .. }| {
                    let name = format!("\"{ident}\"");
                    quote!(#name, <#ty as #macro_util::PgType>::SQL_NAME)
                })
                .collect::<Punctuated<_, Token![,]>>();

            let ret = match &output {
                ReturnType::Default => quote!("VOID"),
                ReturnType::Type(_, ty) => quote!(<#ty as #macro_util::PgType>::SQL_NAME),
                ReturnType::Table { columns, .. } => {
                    let columns = columns
                        .iter()
                        .map(|LambdaArg { ident, ty, .. }| {
                            let name = format!("\"{ident}\"");
                            quote!(#name, <#ty as #macro_util::PgType>::SQL_NAME)
                        })
                        .collect::<Punctuated<_, Token![,]>>();
                    quote!("TABLE (", #columns, ")")
                },
            };

            let create_statement = quote! {
                #macro_util::concat!(
                    "CREATE OR REPLACE FUNCTION \"",
                    #name,
                    "\"(",
                    #args,
                    ") RETURNS ",
                    #ret,
                    "LANGUAGE PLPGSQL AS $$ ",
                    #body,
                    " $$;"
                );
            };

            (name, create_statement, select_statement)
        };

        let (lifetime, generics) = match inputs.len() {
            0 => (quote!('static), None),
            _ => (quote!('a), Some(quote!(<'a>))),
        };

        let output = match &output {
            ReturnType::Default => quote!(-> #macro_util::PgLambda<#lifetime, ()>),
            ReturnType::Type(rarrow, ty) => quote!(#rarrow #macro_util::PgLambda<#lifetime, #ty>),
            ReturnType::Table { rarrow, columns, .. } => {
                let tuple = columns
                    .iter()
                    .map(|arg| &arg.ty)
                    .collect::<Punctuated<_, Token![,]>>();
                quote!(#rarrow #macro_util::PgLambda<#lifetime, (#tuple,)>)
            },
        };

        let input_idents = inputs
            .iter()
            .map(|arg| arg.ident.clone())
            .collect::<Punctuated<_, Token![,]>>();

        let inputs = inputs
            .into_iter()
            .map(|LambdaArg { ident, colon_token, ty }| quote!(#ident #colon_token &#lifetime impl #macro_util::ToPgValue<#ty>))
            .collect::<Punctuated<_, Token![,]>>();

        tokens.push(quote! {
            #(#attrs)*
            #vis #fn_token #ident #generics ( #inputs ) #output {
                #macro_util::PgLambda::new(#select_statement, Box::new([#input_idents]))
            }
        });
    }

    Ok(quote! {
        #(#tokens)*
    })
}
