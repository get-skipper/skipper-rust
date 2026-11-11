//! `#[skipper]`: re-exported by the `get-skipper` crate, use it from there.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn, ReturnType, Type};

/// Skip the test when it is disabled in the Skipper spreadsheet.
///
/// Put it on any test function, above or below `#[test]`, `#[tokio::test]`
/// or `#[rstest]`. The test ID is `file > module::path > fn_name`.
/// Tests returning `Result<(), E>` return `Ok(())` when skipped.
#[proc_macro_attribute]
pub fn skipper(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[skipper] takes no arguments",
        )
        .to_compile_error()
        .into();
    }
    let mut f = parse_macro_input!(item as ItemFn);
    let ret = match &f.sig.output {
        ReturnType::Default => quote!(return;),
        ReturnType::Type(_, ty) if is_result(ty) => quote!(return ::core::result::Result::Ok(());),
        ReturnType::Type(_, ty) => {
            return syn::Error::new_spanned(
                ty,
                "#[skipper] supports tests returning () or Result<(), E>",
            )
            .to_compile_error()
            .into()
        }
    };
    let name = f.sig.ident.to_string();
    let check: syn::Stmt = syn::parse_quote! {
        if ::get_skipper::__should_skip(::core::file!(), ::core::module_path!(), #name) {
            #ret
        }
    };
    f.block.stmts.insert(0, check);
    quote!(#f).into()
}

fn is_result(ty: &Type) -> bool {
    matches!(ty, Type::Path(p) if p.path.segments.last().is_some_and(|s| s.ident == "Result"))
}
