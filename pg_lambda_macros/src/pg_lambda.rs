use md5::{Digest, Md5};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{ToTokens, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::token::Paren;
use syn::{
    Attribute, Expr, ExprPath, Ident, LitStr, Path, Result, Token, Type, TypePath, Visibility,
    braced, parenthesized,
};

use crate::{query_params, util};

struct LambdaArg {
    ident: Ident,
    colon_token: Token![:],
    ty: Type,
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
    Type(Type),
    Table(Punctuated<LambdaArg, Token![,]>),
}

impl Parse for ReturnType {
    fn parse(input: ParseStream) -> Result<Self> {
        if !input.peek(Token![->]) {
            return Ok(Self::Default);
        }
        input.parse::<Token![->]>()?;
        let ty = input.parse::<Type>()?;
        Ok(match ty {
            Type::Path(TypePath { path, .. }) if path.is_ident("Table") => {
                let content;
                braced!(content in input);
                Self::Table(Punctuated::parse_terminated(&content)?)
            }
            _ => Self::Type(ty),
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
    let macro_util = util::macro_util_path();

    let mut tokens = Vec::new();

    for Lambda {
        attrs,
        vis,
        fn_token,
        ident,
        inputs,
        output,
        block,
        ..
    } in input.items
    {
        let (name, create_statement, select_statement) = {
            let body = block.value();

            let md5 = {
                let mut hasher = Md5::new();
                hasher.update(ident.to_string());
                hasher.update(&body);
                for input in &inputs {
                    hasher.update(input.ident.to_string());
                    hasher.update(input.ty.to_token_stream().to_string());
                }
                match &output {
                    ReturnType::Default => (),
                    ReturnType::Type(ty) => hasher.update(ty.to_token_stream().to_string()),
                    ReturnType::Table(columns) => {
                        for column in columns {
                            hasher.update(column.ident.to_string());
                            hasher.update(column.ty.to_token_stream().to_string());
                        }
                    }
                }
                u128::from_be_bytes(hasher.finalize().as_slice().try_into().unwrap())
            };

            let name = format!("{ident}_{md5}");

            let select_params = (1..=inputs.len())
                .map(|i| format!("${i}"))
                .collect::<Vec<_>>()
                .join(",");

            let select_statement = format!("SELECT \"{name}\"({select_params});");

            let args = inputs.iter().map(|LambdaArg { ident, ty, .. }| {
                let name = format!("\"{ident}\" ");
                quote!(#name, <#ty as #macro_util::PgType>::SQL_NAME)
            });

            let ret = match &output {
                ReturnType::Default => quote!("VOID"),
                ReturnType::Type(ty) => quote!(<#ty as #macro_util::PgType>::SQL_NAME),
                ReturnType::Table(columns) => {
                    let columns = columns.iter().map(|LambdaArg { ident, ty, .. }| {
                        let name = format!("\"{ident}\" ");
                        quote!(#name, <#ty as #macro_util::PgType>::SQL_NAME)
                    });
                    quote!("TABLE (", #(#columns,)* ")")
                }
            };

            let create_statement = quote! {
                #macro_util::concat!(
                    "CREATE FUNCTION \"",
                    #name,
                    "\"(",
                    #(#args,)*
                    ") RETURNS ",
                    #ret,
                    " LANGUAGE PLPGSQL AS $$ BEGIN ",
                    #body,
                    " END; $$;",
                )
            };

            (name, create_statement, select_statement)
        };

        let (lifetime, generics) = match inputs.len() {
            0 => (quote!('static), None),
            _ => (quote!('a), Some(quote!(<'a>))),
        };

        let ret = match output {
            ReturnType::Default => quote!(()),
            ReturnType::Type(ty) => quote!(#ty),
            ReturnType::Table(columns) => {
                let tuple = columns.iter().map(|arg| &arg.ty);
                quote!(#macro_util::AnonymousTable<(#(#tuple,)*)>)
            }
        };

        let output =
            quote!(-> #macro_util::PgLambda<impl #macro_util::QueryParams + #lifetime, #ret>);

        let params = query_params::from_expr_type_pairs(
            &macro_util,
            inputs
                .iter()
                .map(|LambdaArg { ident, ty, .. }| {
                    let ty = ty.clone();
                    let expr = Expr::Path(ExprPath {
                        attrs: Vec::new(),
                        qself: None,
                        path: Path::from(ident.clone()),
                    });
                    (ty, expr)
                })
                .collect(),
        );

        let inputs = inputs.iter().map(
            |LambdaArg {
                 ident,
                 colon_token,
                 ty,
             }| {
                quote!(#ident #colon_token &#lifetime impl #macro_util::ToPgValue<#ty>,)
            },
        );

        tokens.push(quote! {
            #(#attrs)*
            #vis #fn_token #ident #generics (#(#inputs)*) #output {
                #macro_util::new_lambda(#select_statement, #params)
            }

            #macro_util::submit! {
                #macro_util::PgLambdaDef {
                    name: #name,
                    create_statement: #create_statement,
                }
            }
        });
    }

    Ok(quote! {
        #(#tokens)*
    })
}
