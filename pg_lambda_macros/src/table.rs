use std::collections::HashMap;

use pest::iterators::Pair;
use pest::Parser;
use pest_derive::Parser;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Attribute, Expr, Fields, Ident, ItemStruct, Lit, LitStr, Meta, Result, Token};

use crate::util::{self, bail, ensure, ensure_let};

fn is_attr_ours(attr: &Attribute) -> bool {
    matches!(&attr.meta, Meta::List(list) if list.path.get_ident().is_some_and(|ident| ident == "table"))
}

fn check_ident(ident: &Ident) -> Result<String> {
    let value = ident.to_string();
    ensure!(
        AttributeValueParser::parse(Rule::Ident, &value).is_ok(),
        ident.span(),
        "invalid table name, only ascii alphanumeric characters and underscores are allowed: `[_a-zA-Z][_a-zA-Z0-9]`",
    );
    Ok(value)
}

#[derive(Parser)]
#[grammar = "table.pest"]
struct AttributeValueParser;

fn extract_ident_tuple(pair: Pair<Rule>) -> Vec<String> {
    pair.into_inner()
        .map(|pair| pair.as_str().to_owned())
        .collect()
}

fn extract_referential_action(pair: Option<Pair<Rule>>) -> ReferentialAction {
    match pair {
        Some(pair) => {
            let mut pairs = pair.into_inner();
            match pairs.next().unwrap().as_rule() {
                Rule::NoAction => ReferentialAction::NoAction,
                Rule::Restrict => ReferentialAction::Restrict,
                Rule::Cascade => ReferentialAction::Cascade,
                Rule::SetNull => ReferentialAction::SetNull {
                    columns: extract_ident_tuple(pairs.next().unwrap()),
                },
                Rule::SetDefault => ReferentialAction::SetDefault {
                    columns: extract_ident_tuple(pairs.next().unwrap()),
                },
                _ => unreachable!(),
            }
        }
        None => ReferentialAction::NoAction,
    }
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

struct References {
    table: String,
    ref_columns: Vec<String>,
    on_delete: ReferentialAction,
}

enum ContainerAttribute {
    Schema {
        schema: String,
    },
    PrimaryKey {
        columns: Vec<String>,
    },
    ForeignKey {
        columns: Vec<String>,
        references: References,
    },
    Check {
        expr: String,
    },
    Unique {
        columns: Vec<String>,
        nulls_not_distinct: bool,
    },
}

impl ContainerAttribute {
    fn from_metas(Metas(metas): Metas) -> Result<Vec<Self>> {
        let mut res = vec![];
        for meta in metas {
            // TODO
        }
        Ok(res)
    }
}

enum FieldAttribute {
    PrimaryKey,
    ForeignKey { references: References },
    Unique { nulls_not_distinct: bool },
}

impl FieldAttribute {
    fn from_metas(Metas(metas): Metas) -> Result<Vec<Self>> {
        let mut res = vec![];
        for (ident, lit) in metas {
            res.push(match (ident.to_string().as_str(), lit) {
                ("primary_key", None) => Self::PrimaryKey,
                ("primary_key", Some(lit)) => bail!(lit.span(), "expected no string value"),
                ("foreign_key", None) => bail!(ident.span(), "expected a string value in format `\"<table_name> (<refcolumn>) <delete_action>?\"`"),
                ("foreign_key", Some(lit)) => {
                    let src = lit.value();
                    ensure_let!(
                        Ok(mut pairs) = AttributeValueParser::parse(Rule::ColumnForeignKey, &src),
                        lit.span(),
                        "failed to parse foreign key descriptor, correct format is `\"<table_name> (<refcolumn>) <delete_action>?\"`",
                    );

                    let references = References {
                        table: pairs.next().unwrap().as_str().to_owned(),
                        ref_columns: vec![pairs.next().unwrap().as_str().to_owned()],
                        on_delete: extract_referential_action(pairs.next()),
                    };

                    Self::ForeignKey { references }
                },
                ("unique", None) => Self::Unique { nulls_not_distinct: false },
                ("unique", Some(lit)) => {
                    let src = lit.value();
                    ensure!(
                        AttributeValueParser::parse(Rule::ColumnUnique, &src).is_ok(),
                        lit.span(),
                        "expected either no string value or `\"nulls_not_distinct\"`",
                    );
                    Self::Unique { nulls_not_distinct: true }
                },
                _ => bail!(ident.span(), "unknown table attribute option, expected one of `\"primary_key\"`, `\"foreign_key\"` or `\"unique\"`"),
            });
        }
        Ok(res)
    }
}

pub fn main(input: TokenStream) -> Result<TokenStream2> {
    let item = syn::parse::<ItemStruct>(input)?;
    let this = util::crate_ident();
    let ident = &item.ident;

    // Schema data.
    let name = check_ident(&item.ident)?;
    let mut schema = None::<String>;
    let mut columns = HashMap::new();
    let mut primary_key = None::<Vec<String>>;
    let mut constraints = Vec::<TokenStream2>::default();

    ensure_let!(
        Fields::Named(fields) = &item.fields,
        item.fields.span(),
        "expected a struct with named fields"
    );

    for field in &fields.named {
        let name = check_ident(field.ident.as_ref().unwrap())?;
        let ty = &field.ty;
        columns.insert(name, quote!(Cow::Borrowed(<#ty as PgType>::SQL_NAME)));
    }

    for attr in &item.attrs {
        if is_attr_ours(attr) {
            for attr in attr
                .parse_args::<Metas>()
                .and_then(ContainerAttribute::from_metas)?
            {}
        }
    }

    for attr in fields.named.iter().flat_map(|field| field.attrs.iter()) {
        if is_attr_ours(attr) {
            for attr in attr
                .parse_args::<Metas>()
                .and_then(FieldAttribute::from_metas)?
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
                        name: Name::new(#name),
                    },
                    columns: Cow::Borrowed(&[]),
                    constraints: Cow::Borrowed(&[]),
                };
            }

            submit!(&#ident::SCHEMA);
        };
    })
}
