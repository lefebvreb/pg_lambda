use proc_macro::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::{Attribute, ItemStruct, Meta, Result};

mod util;

#[proc_macro_derive(Table, attributes(table))]
pub fn table(input: TokenStream) -> TokenStream {
    fn get_attr_tokens(attr: &Attribute) -> Option<proc_macro2::TokenStream> {
        match &attr.meta {
            Meta::List(metalist) if metalist.path.get_ident().is_some_and(|ident| ident == "table") => {
                Some(metalist.tokens.clone())
            },
            _ =>  None,
        }
    }

    pub enum TableAttribute {
        Check(String),
    }

    impl Parse for TableAttribute {
        fn parse(input: ParseStream) -> Result<Self> {
            Ok(Self::Check("TRUE".to_string()))
        }
    }

    pub enum ColumnAttribute {
        
    }

    impl Parse for ColumnAttribute {
        fn parse(input: ParseStream) -> Result<Self> {
            todo!()
        }
    }

    util::syn_try(|| {
        let input = syn::parse::<ItemStruct>(input)?;
        let this = util::crate_ident();

        let ty_name = input.ident;
        let table_name = ty_name.to_string();

        for attr in input.attrs {
            if let Some(tokens) = get_attr_tokens(&attr) {
                std::fs::write(".vscode/logs.txt", format!("{}", tokens.to_string())).unwrap();
                // let attr = syn::parse2::<TableAttribute>(tokens)?;
            }
        }

        Ok(quote! {
            const _: () = {
                use ::#this::__proc_macro_util::*;

                impl Table for #ty_name {
                    const SCHEMA: TableSchema = TableSchema {
                        name: TableName {
                            schema: None,
                            name: Cow::Borrowed(#table_name)
                        },
                        columns: Cow::Borrowed(&[]),
                        constraints: Cow::Borrowed(&[]),
                    };
                }
    
                submit!(&#ty_name::SCHEMA);
            };
        })
    })
} 

#[proc_macro]
pub fn plpgsql(input: TokenStream) -> TokenStream {
    util::syn_try(|| Ok(quote! {}))
}
