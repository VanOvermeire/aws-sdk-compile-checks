use proc_macro2::Ident;
use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Comma;
use syn::Token;

pub(crate) mod kw {
    syn::custom_keyword!(sdk);
}

#[derive(Debug)]
pub struct Attributes {
    pub span: Span,
    pub sdks: Vec<String>,
}

impl Parse for Attributes {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Attributes {
                span: input.span(),
                sdks: vec![],
            });
        }

        let sdk_keyword: kw::sdk = input
            .parse()
            .map_err(|_| syn::Error::new(input.span(), "the only allowed attribute is `sdk`"))?;
        let _equals_token: Token![=] = input.parse().map_err(|_| {
            syn::Error::new(
                sdk_keyword.span(),
                "expected `sdk` to be followed by a `=` and one or more SDKs, e.g. `sdk = sqs`",
            )
        })?;
        let sdks: Punctuated<Ident, Comma> = Punctuated::parse_terminated(input).map_err(|_| {
            syn::Error::new(
                input.span(),
                "expected one or more SDKs, separated by `,` after keyword `sdk`, e.g. `sdk = sqs,s3`",
            )
        })?;

        if sdks.is_empty() {
            return Err(syn::Error::new(
                sdk_keyword.span(),
                "expected one or more SDKs, separated by `,` after keyword `sdk`, e.g. `sdk = sqs,s3`",
            ));
        }

        Ok(Attributes {
            span: input.span(),
            sdks: sdks.iter().map(|ident| ident.to_string()).collect(),
        })
    }
}
