use indexmap::IndexMap;
use pest::Parser;
use pest::iterators::{Pair, Pairs};
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

fn extract_column_tuple(pair: Pair<Rule>) -> Vec<String> {
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
                    columns: extract_column_tuple(pairs.next().unwrap()),
                },
                Rule::SetDefault => ReferentialAction::SetDefault {
                    columns: extract_column_tuple(pairs.next().unwrap()),
                },
                _ => unreachable!(),
            }
        }
        None => ReferentialAction::NoAction,
    }
}

fn extract_references(mut pairs: Pairs<Rule>) -> References {
    let (table, schema) = {
        let mut pairs = pairs.next().unwrap().into_inner();
        let one = pairs.next().unwrap().as_str().to_owned();
        match pairs.next() {
            Some(two) => (two.as_str().to_owned(), Some(one)),
            None => (one, None),
        }
    };

    let ref_columns = pairs
        .next()
        .unwrap()
        .into_inner()
        .map(|column| column.as_str().to_owned())
        .collect();

    References {
        table,
        schema,
        ref_columns,
        on_delete: extract_referential_action(pairs.next()),
    }
}

fn check_ident(ident: &Ident) -> Result<String> {
    let value = ident.to_string();
    ensure!(
        AttributeValueParser::parse(Rule::Ident, &value).is_ok(),
        ident.span(),
        "table name must be a valid rust identifier",
    );
    Ok(value)
}

fn is_attribute_ours(attr: &Attribute) -> bool {
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

fn table_name_tokens(table: String, schema: Option<String>) -> TokenStream2 {
    let schema = schema.map_or_else(|| quote!(None), |schema| quote!(Some(Name::new(#schema))));
    quote! {
        TableName {
            schema: #schema,
            name: Name::new(#table),
        }
    }
}

fn name_tuple_tokens(columns: Vec<String>) -> TokenStream2 {
    quote! {
        NameTuple(Cow::Borrowed(const {
            &[#(Name::new(#columns),)*]
        }))
    }
}

fn foreign_key_tokens(columns: Vec<String>, references: References) -> TokenStream2 {
    let columns = name_tuple_tokens(columns);
    let table = table_name_tokens(references.table, references.schema);
    let ref_columns = name_tuple_tokens(references.ref_columns);
    let on_delete = match references.on_delete {
        ReferentialAction::NoAction => quote!(ReferentialAction::NoAction),
        ReferentialAction::Restrict => quote!(ReferentialAction::Restrict),
        ReferentialAction::Cascade => quote!(ReferentialAction::Cascade),
        ReferentialAction::SetNull { columns } => {
            let columns = name_tuple_tokens(columns);
            quote! {
                ReferentialAction::SetNull {
                    columns: #columns,
                }
            }
        }
        ReferentialAction::SetDefault { columns } => {
            let columns = name_tuple_tokens(columns);
            quote! {
                ReferentialAction::SetDefault {
                    columns: #columns,
                }
            }
        }
    };

    quote! {
        Constraint::ForeignKey {
            columns: #columns,
            table: #table,
            ref_columns: #ref_columns,
            on_delete: #on_delete,
        }
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
    schema: Option<String>,
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
    fn from_meta(ParsedMeta { name, value }: &ParsedMeta) -> Result<Self> {
        Ok(match (name.to_string().as_str(), value) {
            ("schema", None) => bail!(
                name.span(),
                "expected a schema name: `schema = \"my_schema\"`"
            ),
            ("schema", Some(lit)) => {
                let src = lit.value();
                ensure_let!(
                    Ok(mut pairs) = AttributeValueParser::parse(Rule::Ident, &src),
                    lit.span(),
                    "schema name must be a valid rust identifier",
                );
                Self::Schema {
                    name: pairs.next().unwrap().as_str().to_owned(),
                }
            }
            ("primary_key", None) => bail!(
                name.span(),
                "expected a value: `primary_key = \"(<column>, …)\"`",
            ),
            ("primary_key", Some(lit)) => {
                let src = lit.value();
                ensure_let!(
                    Ok(mut pairs) = AttributeValueParser::parse(Rule::ContainerPrimaryKey, &src),
                    lit.span(),
                    "failed to parse primary key description, correct format is `\"(<column>, …)\"`",
                );
                Self::PrimaryKey {
                    columns: extract_column_tuple(pairs.next().unwrap()),
                }
            }
            ("foreign_key", None) => bail!(
                name.span(),
                "expected a foreign key constraint description: `foreign_key = \"(<column>, …) <table_name> (<refcolumn>, …) <delete_action>?\"`",
            ),
            ("foreign_key", Some(lit)) => {
                let src = lit.value();
                ensure_let!(
                    Ok(mut pairs) = AttributeValueParser::parse(Rule::ContainerForeignKey, &src),
                    lit.span(),
                    "failed to parse foreign key constraint description, correct format is `\"(<column>, …) <table_name> (<refcolumn>, …) <delete_action>?\"`",
                );
                let columns = extract_column_tuple(pairs.next().unwrap());
                let references = extract_references(pairs);
                Self::ForeignKey {
                    columns,
                    references,
                }
            }
            ("check", None) => bail!(
                name.span(),
                "expected a check expression: `check = \"<sql_expression>\"`",
            ),
            ("check", Some(lit)) => Self::Check { expr: lit.value() },
            ("unique", None) => bail!(
                name.span(),
                "expected a unique constraint description: `unique = \"(<column>, …) <nulls_not_distinct>?\"`",
            ),
            ("unique", Some(lit)) => {
                let src = lit.value();
                ensure_let!(
                    Ok(mut pairs) = AttributeValueParser::parse(Rule::ContainerUnique, &src),
                    lit.span(),
                    "failed to parse unique constraint description: `unique = \"(<column>, …) <nulls_not_distinct>?\"`",
                );
                let columns = extract_column_tuple(pairs.next().unwrap());
                let nulls_not_distinct = pairs.next().is_some();
                Self::Unique {
                    columns,
                    nulls_not_distinct,
                }
            }
            _ => bail!(
                name.span(),
                "unknown table derive option, expected one of `\"schema\"`, `\"primary_key\"`, `\"foreign_key\"`, `\"check\"` or `\"unique\"`"
            ),
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
            ("primary_key", Some(lit)) => bail!(lit.span(), "expected no value"),
            ("foreign_key", None) => bail!(
                name.span(),
                "expected a foreign key constraint description: `foreign_key = \"<table_name> (<refcolumn>) <delete_action>?\"`"
            ),
            ("foreign_key", Some(lit)) => {
                let src = lit.value();
                ensure_let!(
                    Ok(pairs) = AttributeValueParser::parse(Rule::FieldForeignKey, &src),
                    lit.span(),
                    "failed to parse foreign key constraint description, correct format is `\"<table_name> (<refcolumn>) <delete_action>?\"`",
                );
                let references = extract_references(pairs);
                Self::ForeignKey { references }
            }
            ("unique", None) => Self::Unique {
                nulls_not_distinct: false,
            },
            ("unique", Some(lit)) => {
                let src = lit.value();
                ensure!(
                    AttributeValueParser::parse(Rule::FieldUnique, &src).is_ok(),
                    lit.span(),
                    "expected either no string value or `\"nulls_not_distinct\"`",
                );
                Self::Unique {
                    nulls_not_distinct: true,
                }
            }
            _ => bail!(
                name.span(),
                "unknown table derive option, expected one of `\"primary_key\"`, `\"foreign_key\"` or `\"unique\"`"
            ),
        })
    }
}

pub fn main(input: TokenStream) -> Result<TokenStream2> {
    let item = syn::parse::<ItemStruct>(input)?;
    let this = util::crate_ident();
    let ident = &item.ident;

    // Schema data.
    let table = check_ident(&item.ident)?;
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
            if !is_attribute_ours(attr) {
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
                        primary_key = Some(vec![column.clone()]);
                    }
                    FieldAttribute::ForeignKey { references } => {
                        constraints.push(foreign_key_tokens(vec![column.clone()], references));
                    }
                    FieldAttribute::Unique { nulls_not_distinct } => {
                        let columns = name_tuple_tokens(vec![column.clone()]);
                        constraints.push(quote! {
                            Constraint::Unique {
                                columns: #columns,
                                nulls_not_distinct: #nulls_not_distinct,
                            }
                        });
                    }
                }
            }
        }

        let ty = &field.ty;
        columns.insert(column, quote!(Cow::Borrowed(<#ty as PgType>::SQL_NAME)));
    }

    for attr in &item.attrs {
        if !is_attribute_ours(attr) {
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
                }
                ContainerAttribute::ForeignKey {
                    columns,
                    references,
                } => {
                    constraints.push(foreign_key_tokens(columns, references));
                }
                ContainerAttribute::Check { expr } => {
                    constraints.push(quote! {
                        Constraint::Check {
                            expr: Cow::Borrowed(#expr),
                        }
                    });
                }
                ContainerAttribute::Unique {
                    columns,
                    nulls_not_distinct,
                } => {
                    let columns = name_tuple_tokens(columns);
                    constraints.push(quote! {
                        Constraint::Unique {
                            columns: #columns,
                            nulls_not_distinct: #nulls_not_distinct,
                        }
                    });
                }
            }
        }
    }

    let name = table_name_tokens(table, schema);
    let columns = columns.into_iter().map(|(name, ty)| {
        quote! {
            Column {
                name: Name::new(#name),
                ty: #ty,
            }
        }
    });
    if let Some(primary_key) = primary_key {
        let primary_key = name_tuple_tokens(primary_key);
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
                const SCHEMA: TableSchema<'static> = TableSchema {
                    name: #name,
                    columns: Cow::Borrowed(const {
                        &[#(#columns,)*]
                    }),
                    constraints: Cow::Borrowed(const {
                        &[#(#constraints,)*]
                    }),
                };
            }
            submit! {
                TableDef {
                    schema: &<#ident as Table>::SCHEMA,
                }
            };
        };
    })
}
