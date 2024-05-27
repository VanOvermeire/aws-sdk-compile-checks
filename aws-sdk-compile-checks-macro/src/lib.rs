use proc_macro::TokenStream;

use quote::quote;
use syn::{parse_macro_input, ItemFn};

use crate::attributes::Attributes;
use crate::findings::UsageFinds;
use crate::required_properties::{create_required_props_map, valid_sdks};

mod attributes;
mod required_properties;
mod visitor;
mod findings;

#[proc_macro_attribute]
pub fn required_props(attrs: TokenStream, input: TokenStream) -> TokenStream {
    let attributes: Attributes = parse_macro_input!(attrs);
    let item: ItemFn = parse_macro_input!(input);
    let required_props = create_required_props_map();

    let Attributes { sdks, span } = attributes;
    match valid_sdks(&required_props, &sdks) {
        Ok(_) => {}
        Err(e) => {
            return syn::Error::new(
                span,
                format!("some of the SDKs you specified do not exist in our list of supported SDKs: {}", e),
            )
            .to_compile_error()
            .into();
        }
    }

    let visitor = visitor::MethodVisitor::new(&item, required_props);
    let improper = visitor.find_improper_usages(sdks);

    let errors: Vec<proc_macro2::TokenStream> = improper
        .into_iter()
        .map(UsageFinds::into_compile_error)
        .collect();

    quote!(
        #(#errors)*
        #item
    )
    .into()
}
