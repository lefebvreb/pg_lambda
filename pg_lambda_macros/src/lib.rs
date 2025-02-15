use proc_macro::TokenStream;

mod pg_lambda;
mod table;
mod util;

#[proc_macro]
pub fn pg_lambda(input: TokenStream) -> TokenStream {
    util::syn_try(|| pg_lambda::main(input))
}

#[proc_macro_derive(Table, attributes(table))]
pub fn table(input: TokenStream) -> TokenStream {
    util::syn_try(|| table::main(input))
}
