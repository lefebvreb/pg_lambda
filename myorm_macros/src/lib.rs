use proc_macro::TokenStream;
use quote::quote;
use syn::ItemStruct;

mod util;

#[proc_macro_derive(Table)]
pub fn table(input: TokenStream) -> TokenStream {
    util::syn_try(|| {
        let input = syn::parse::<ItemStruct>(input)?;
        let this = util::crate_ident();

        let name = input.ident;

        Ok(quote! {
            impl ::#this::__macros::Table for #name {
                const SCHEMA: ::#this::__macros::TableSchema = ::#this::__macros::TableSchema {
                    // todo
                };
            }

            ::#this::__macros::submit! {
                // doubt this will work
                #name::SCHEMA
            }
        })
    })
} 

#[proc_macro]
pub fn plpgsql(input: TokenStream) -> TokenStream {
    util::syn_try(|| {
        // let input = syn::parse::<ItemFn>(input)?;
        // util::ensure_let!(Some(block) = input.block.span().source_text(), input.block.span(), "");

        Ok(quote! {})
    })
}
