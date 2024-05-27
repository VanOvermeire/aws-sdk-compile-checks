use proc_macro2::{Span, TokenStream};

const COMMA_WITH_SPACE: &str = ", ";

#[derive(Debug)]
pub(crate) enum UsageFinds {
    Improper(ImproperUsage),
    Unknown(UnknownUsage),
}

#[derive(Debug)]
pub(crate) struct UnknownUsage {
    pub(crate) span: Span,
    pub(crate) method: String,
    pub(crate) sdks: Vec<String>,
}

#[derive(Debug)]
pub(crate) struct ImproperUsage {
    pub(crate) span: Span,
    pub(crate) method: String,
    pub(crate) missing: Vec<String>,
    pub(crate) sdk: String,
}

impl UsageFinds {
    pub fn into_compile_error(self) -> TokenStream {
        match self {
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
    }
}
