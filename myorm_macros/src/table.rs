use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Attribute, Expr, Ident, ItemStruct, Lit, LitStr, Meta, Result, Token};

use crate::util::{self, bail, ensure_let};

fn is_attr_ours(attr: &Attribute) -> bool {
    matches!(&attr.meta, Meta::List(list) if list.path.get_ident().is_some_and(|ident| ident == "table"))
}

struct Metas(Vec<(Ident, Option<LitStr>)>);

impl Parse for Metas {
    fn parse(input: ParseStream) -> Result<Self> {
        let metas = Punctuated::<Meta, Token![,]>::parse_terminated(input)?;
        let mut res = vec![];

        for meta in metas {
            match meta {
                Meta::Path(path) => {
                    ensure_let!(
                        Some(ident) = path.get_ident(),
                        path.span(),
                        "expected identifier",
                    );
                    res.push((ident.clone(), None));
                }
                Meta::List(list) => bail!(
                    list.span(),
                    "expected either a `name` or `name = \"value\"` style meta on this attribute",
                ),
                Meta::NameValue(name_value) => {
                    ensure_let!(
                        Some(ident) = name_value.path.get_ident(),
                        name_value.path.span(),
                        "expected identifier",
                    );
                    ensure_let!(
                        Expr::Lit(lit) = name_value.value,
                        name_value.path.span(),
                        "expected literal",
                    );
                    ensure_let!(
                        Lit::Str(str) = lit.lit,
                        lit.span(),
                        "expected string literal",
                    );
                    res.push((ident.clone(), Some(str)));
                }
            }
        }

        Ok(Self(res))
    }
}

enum ReferentialAction {
    NoAction,
    Restrict,
    Cascade,
    SetNull { columns: Vec<String> },
    SetDefault { columns: Vec<String> },
}

struct ForeignKeyConstraint {
    table: Ident,
    foreign_columns: Vec<String>,
    on_delete: ReferentialAction,
}

enum ContainerAttribute {
    Schema {
        schema: String,
    },
    Check {
        expr: String,
    },
    Unique {
        columns: Vec<String>,
        nulls_not_distinct: bool,
    },
    PrimaryKey {
        columns: Vec<String>,
    },
    ForeignKey {
        columns: Vec<String>,
        references: ForeignKeyConstraint,
    },
}

impl ContainerAttribute {
    fn from_metas(Metas(metas): Metas) -> Result<Vec<Self>> {
        Ok(vec![])
    }
}

enum ColumnAttribute {
    Unique { nulls_not_distinct: bool },
    PrimaryKey,
    ForeignKey { references: ForeignKeyConstraint },
}

impl ColumnAttribute {
    fn from_metas(Metas(metas): Metas) -> Result<Vec<Self>> {
        Ok(vec![])
    }
}

impl Parse for ColumnAttribute {
    fn parse(input: ParseStream) -> Result<Self> {
        todo!()
    }
}

pub fn main(item: ItemStruct) -> Result<TokenStream> {
    let this = util::crate_ident();
    let ident = &item.ident;

    let table = item.ident.to_string();
    let mut schema = None::<String>;
    let mut primary_key = None::<Vec<String>>;
    let mut constraints = Vec::<TokenStream>::default();

    for attr in item.attrs {
        if is_attr_ours(&attr) {
            for attr in attr
                .parse_args::<Metas>()
                .and_then(ContainerAttribute::from_metas)?
            {}
        }
    }

    Ok(quote! {
        const _: () = {
            use ::#this::__proc_macro_util::*;

            impl Table for #ident {
                const SCHEMA: TableSchema = TableSchema {
                    name: TableName {
                        schema: None,
                        name: Name::new(#table),
                    },
                    columns: Cow::Borrowed(&[]),
                    constraints: Cow::Borrowed(&[]),
                };
            }

            submit!(&#ident::SCHEMA);
        };
    })
}
