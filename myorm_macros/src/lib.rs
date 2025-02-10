use proc_macro::TokenStream;

mod table;
mod util;

#[proc_macro_derive(Table, attributes(table))]
pub fn table(input: TokenStream) -> TokenStream {
    util::syn_try(|| {
        let item = syn::parse(input)?;
        table::main(item)
    })
}

#[proc_macro]
pub fn plpgsql(_: TokenStream) -> TokenStream {
    util::syn_try(|| todo!())
}
