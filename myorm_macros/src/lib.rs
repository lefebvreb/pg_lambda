use proc_macro::TokenStream;
use quote::quote;
use syn::ItemFn;
use syn::spanned::Spanned;

mod util;

#[proc_macro]
pub fn plpgsql(input: TokenStream) -> TokenStream {
    util::syn_try(|| {
        // let input = syn::parse::<ItemFn>(input)?;
        // util::ensure_let!(Some(block) = input.block.span().source_text(), input.block.span(), "");

        Ok(quote! {})
    })
}
