use indexmap::IndexMap;
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

fn is_attr_ours(attr: &Attribute) -> bool {
    matches!(&attr.meta, Meta::List(list) if list.path.get_ident().is_some_and(|ident| ident == "table"))
}

fn parse_meta_list<T>(
    attr: &Attribute,
    f: fn(&ParsedMeta) -> Result<T>,
) -> Result<Vec<(T, ParsedMeta)>> {
    match &attr.meta {
        Meta::Path(path) => bail!(
            path.span(),
            "expected attribute arguments in parentheses: `#[table(...)]`"
        ),
        Meta::List(list) => {
            let mut res = vec![];
            for meta in
                list.parse_args_with(Punctuated::<ParsedMeta, Token![,]>::parse_terminated)?
            {
                res.push((f(&meta)?, meta));
            }
            Ok(res)
        }
        Meta::NameValue(meta) => bail!(
            meta.eq_token.span(),
            "expected parentheses: `#[table(...)]`"
        ),
    }
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

fn column_tuple(columns: Vec<String>) -> TokenStream2 {
    quote! {
        ColumnTuple(cow_slice(&[
            #(Name::new(#columns),)*
        ]))
    }
}

struct ParsedMeta {
    name: Ident,
    value: Option<LitStr>,
}

impl Parse for ParsedMeta {
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(match input.parse::<Meta>()? {
            Meta::Path(path) => {
                ensure_let!(
                    Some(ident) = path.get_ident(),
                    path.span(),
                    "expected identifier",
                );
                ParsedMeta {
                    name: ident.clone(),
                    value: None,
                }
            }
            Meta::List(list) => bail!(
                list.span(),
                "expected either a `name` or `name = \"value\"` style meta on this attribute",
            ),
            Meta::NameValue(meta) => {
                ensure_let!(
                    Some(ident) = meta.path.get_ident(),
                    meta.path.span(),
                    "expected identifier",
                );
                ensure_let!(
                    Expr::Lit(lit) = meta.value,
                    meta.path.span(),
                    "expected literal",
                );
                ensure_let!(
                    Lit::Str(str) = lit.lit,
                    lit.span(),
                    "expected string literal",
                );
                ParsedMeta {
                    name: ident.clone(),
                    value: Some(str),
                }
            }
        })
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
        name: String,
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
    fn from_meta(meta: &ParsedMeta) -> Result<Self> {
        Ok(Self::Check {
            expr: String::new(),
        })
    }
}

enum FieldAttribute {
    PrimaryKey,
    ForeignKey { references: References },
    Unique { nulls_not_distinct: bool },
}

impl FieldAttribute {
    fn from_meta(ParsedMeta { name, value }: &ParsedMeta) -> Result<Self> {
        Ok(match (name.to_string().as_str(), value) {
            ("primary_key", None) => Self::PrimaryKey,
            ("primary_key", Some(lit)) => bail!(lit.span(), "expected no string value"),
            ("foreign_key", None) => bail!(name.span(), "expected a string value in format `\"<table_name> (<refcolumn>) <delete_action>?\"`"),
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
            _ => bail!(name.span(), "unknown table attribute option, expected one of `\"primary_key\"`, `\"foreign_key\"` or `\"unique\"`"),
        })
    }
}

pub fn main(input: TokenStream) -> Result<TokenStream2> {
    let item = syn::parse::<ItemStruct>(input)?;
    let this = util::crate_ident();
    let ident = &item.ident;

    // Schema data.
    let name = check_ident(&item.ident)?;
    let mut schema = None;
    let mut columns = IndexMap::new();
    let mut primary_key = None;
    let mut constraints = Vec::new();

    ensure_let!(
        Fields::Named(fields) = &item.fields,
        item.fields.span(),
        "expected a struct with named fields"
    );

    for field in &fields.named {
        let column = check_ident(field.ident.as_ref().unwrap())?;

        for attr in &field.attrs {
            if !is_attr_ours(attr) {
                continue;
            }
    
            for (attr, meta) in parse_meta_list(attr, FieldAttribute::from_meta)? {
                match attr {
                    FieldAttribute::PrimaryKey => {
                        ensure!(
                            primary_key.is_none(),
                            meta.name.span(),
                            "only one primary key may be specified by table, remove this attribute"
                        );
                        primary_key = Some(vec![name.clone()]);
                    },
                    FieldAttribute::ForeignKey { references } => {
                        // constraints.push(quote! {
                        //     Constraint::ForeignKey {
                        //         // TODO
                        //     }
                        // });
                    },
                    FieldAttribute::Unique { nulls_not_distinct } => {
                        let columns = column_tuple(vec![name.clone()]);
                        constraints.push(quote! {
                            Constraint::Unique {
                                columns: #columns,
                                nulls_not_distinct: #nulls_not_distinct,
                            }
                        });
                    },
                }
            }
        }

        // let ty = &field.ty;
        // column_types.insert(name, quote!(Cow::Borrowed(<#ty as PgType>::SQL_NAME)));
        columns.insert(column, quote!(Cow::Borrowed("INTEGER")));
    }

    for attr in &item.attrs {
        if !is_attr_ours(attr) {
            continue;
        }

        for (attr, meta) in parse_meta_list(attr, ContainerAttribute::from_meta)? {
            match attr {
                ContainerAttribute::Schema { name } => {
                    ensure!(
                        schema.is_none(),
                        meta.name.span(),
                        "only one schema may be specified by table, remove this attribute"
                    );
                    schema = Some(name);
                }
                ContainerAttribute::PrimaryKey { columns } => {
                    ensure!(
                        primary_key.is_none(),
                        meta.name.span(),
                        "only one primary key may be specified by table, remove this attribute"
                    );
                    primary_key = Some(columns);
                },
                ContainerAttribute::ForeignKey {
                    columns,
                    references,
                } => {
                    // constraints.push(quote! {
                    //     Constraint::ForeignKey {
                    //         // TODO
                    //     }
                    // });
                },
                ContainerAttribute::Check { expr } => {
                    constraints.push(quote! {
                        Constraint::Check {
                            expr: Cow::Borrowed(#expr),
                        }
                    });
                },
                ContainerAttribute::Unique {
                    columns,
                    nulls_not_distinct,
                } => {
                    let columns = column_tuple(columns);
                    constraints.push(quote! {
                        Constraint::Unique {
                            columns: #columns,
                            nulls_not_distinct: #nulls_not_distinct,
                        }
                    });
                },
            }
        }
    }

    let schema = schema.map_or_else(|| quote!(None), |schema| quote!(Name::new(#schema)));
    let columns = columns.into_iter().map(|(name, ty)| quote! {
        Column {
            name: Name::new(#name),
            ty: #ty,
        }
    });
    if let Some(primary_key) = primary_key {
        let primary_key = column_tuple(primary_key);
        constraints.push(quote! {
            Constraint::PrimaryKey {
                columns: #primary_key,
            }
        });
    }

    Ok(quote! {
        const _: () = {
            use ::#this::__proc_macro_util::*;

            impl Table for #ident {
                const SCHEMA: TableSchema = TableSchema {
                    name: TableName {
                        schema: #schema,
                        name: Name::new(#name),
                    },
                    columns: cow_slice(&[
                        #(#columns,)*
                    ]),
                    constraints: cow_slice(&[
                        #(#constraints,)*
                    ]),
                };
            }

            submit!(&#ident::SCHEMA);
        };
    })
}
