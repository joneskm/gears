use darling::{
    util::{Flag, PathList},
    FromAttributes, FromMeta,
};
use quote::quote;
use syn::{DataStruct, DeriveInput, Field, TypePath};

#[derive(FromMeta)]
#[darling(rename_all = "lowercase")]
enum Kind {
    Sint32,
    Int64,
    Uint32,
    Uint64,
    Bool,
    String,
    Bytes,
}

impl Kind {
    fn to_prost_token(&self) -> proc_macro2::TokenStream {
        match self {
            Kind::Sint32 => quote! { int32 },
            Kind::Int64 => quote! { int64 },
            Kind::Uint32 => quote! { uint32 },
            Kind::Uint64 => quote! { uint64 },
            Kind::Bool => quote! { r#bool },
            Kind::String => quote! { string },
            Kind::Bytes => quote! { bytes },
        }
    }
}

#[derive(FromMeta, Default)]
#[darling(and_then = Self::not_both)]
struct OptionalOrRequired {
    optional: Flag,
    repeated: Flag,
}

impl OptionalOrRequired {
    fn not_both(self) -> darling::Result<Self> {
        if self.optional.is_present() && self.repeated.is_present() {
            Err(
                darling::Error::custom("Cannot set `optional` and `repeated`")
                    .with_span(&self.repeated.span()),
            )
        } else {
            Ok(self)
        }
    }
}

#[derive(FromAttributes)]
#[darling(attributes(proto))]
struct ProtobufAttr {
    #[darling(default)]
    raw: Option<syn::Path>,
    #[darling(flatten, default)]
    opt: OptionalOrRequired,
    kind: Kind,
    #[darling(default)]
    tag: Option<u32>,
}

pub fn extend_new_structure(
    DeriveInput {
        ident, data, vis, ..
    }: DeriveInput,
    raw_derives: PathList,
) -> syn::Result<proc_macro2::TokenStream> {
    match data {
        syn::Data::Struct(DataStruct { fields, .. }) => {
            let mut result_fields = Vec::with_capacity(fields.len());
            let mut counter = 1;

            for Field {
                attrs,
                vis,
                ident,
                ty,
                ..
            } in fields
            {
                let ProtobufAttr {
                    raw,
                    opt: OptionalOrRequired { optional, repeated },
                    kind,
                    tag,
                } = ProtobufAttr::from_attributes(&attrs)?;

                let raw = raw
                    .map(|path| syn::Type::Path(TypePath { qself: None, path }))
                    .unwrap_or(ty);
                let tag = tag.unwrap_or(counter);

                let kind = kind.to_prost_token();

                let result = match (optional.is_present(), repeated.is_present()) {
                    (true, true) => unreachable!("we validated structure to omit such case"),
                    (true, false) => quote! {
                        #[prost( #kind, optional, tag = #tag )]
                        #vis #ident : ::std::option::Option<#raw>,
                    },
                    (false, true) => quote! {
                       #[prost( #kind, repeated, tag = #tag )]
                       #vis #ident : std::vec::Vec<#raw>,
                    },
                    (false, false) => quote! {
                        #[prost( #kind, required, tag = #tag )]
                        #vis #ident : #raw
                    },
                };

                result_fields.push(result);

                counter = tag;
                counter += 1;
            }

            let new_name = syn::Ident::new(
                &format!("Raw{}", ident.to_string()),
                proc_macro2::Span::call_site(),
            );

            let raw_derives = match raw_derives.is_empty() {
                true => quote! {},
                false => quote! { #[derive(#(#raw_derives,)*)] },
            };

            let gen = quote! {

                #[derive(::prost::Message)]
                #raw_derives
                #vis struct  #new_name
                {
                    #(#result_fields),*
                }
            };

            Ok(gen.into())
        }
        _ => Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "Protobuf can be derived only for `struct`",
        )),
    }
}