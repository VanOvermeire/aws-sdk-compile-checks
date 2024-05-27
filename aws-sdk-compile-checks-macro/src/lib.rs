use proc_macro::TokenStream;

use quote::quote;
use syn::{parse_macro_input, ItemFn};

use crate::attributes::Attributes;
use crate::required_properties::{create_required_props_map, valid_sdks};
use crate::visitor::UsageFinds;

mod attributes;
mod required_properties;
mod visitor;

const COMMA_WITH_SPACE: &str = ", ";

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
        .map(|find| {
            match find {
                // TODO move this and the usage finds to a separate file
                UsageFinds::Improper(improper) => {
                    let missing = improper.missing.into_iter().map(|s| format!("`{}`", s)).collect::<Vec<_>>().join(COMMA_WITH_SPACE);
                    let message = format!("method `{}` (from {}) is missing required argument(s): {}", improper.method, improper.sdk, missing);
                    syn::Error::new(improper.span, message)
                        .to_compile_error()
                }
                UsageFinds::Unknown(mut unknown) => {
                    unknown.sdks.sort(); // to have a deterministic output
                    let sdks_to_show = if unknown.sdks.len() <= 5 {
                        unknown.sdks.join(COMMA_WITH_SPACE)
                    } else {
                        format!("{}... (abbreviated list)", unknown.sdks[0..5].join(COMMA_WITH_SPACE))
                    };
                    let first_sdk_option = unknown.sdks.first()
                        .map(|s| s.as_ref())
                        .unwrap_or_else(|| "sqs");
                    let message = format!("method `{}` is used in multiple SDKs: {}. Please add the right one(s) to the attribute, e.g. `#[required_props(sdk = {})]`", unknown.method, sdks_to_show, first_sdk_option);
                    syn::Error::new(unknown.span, message)
                        .to_compile_error()
                }
            }
        }).collect();

    quote!(
        #(#errors)*
        #item
    )
    .into()
}
