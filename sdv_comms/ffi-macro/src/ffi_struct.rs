// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//      http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Implementation of the #[comms_stack_ffi_struct] macro.

use proc_macro2::TokenStream;
use quote::{quote, ToTokens};
use syn::parse::{Error as SynError, Parse, ParseStream, Result as SynResult};
use syn::spanned::Spanned;
use syn::{parse_quote, Attribute, Fields, ItemStruct, Meta, Type};

const OPAQUE_STRUCT_MACRO: &str = "#[comms_stack_ffi_struct(opaque)]";
const SHARED_STRUCT_MACRO: &str = "#[comms_stack_ffi_struct(shared)]";

mod token {
    syn::custom_keyword!(opaque);
    syn::custom_keyword!(shared);
}

#[allow(dead_code)] // the token field is never read but nonetheless required
#[derive(Debug, Clone)]
pub(crate) enum FfiStructKind {
    Opaque(token::opaque),
    Shared(token::shared),
}

impl Parse for FfiStructKind {
    fn parse(input: ParseStream) -> SynResult<Self> {
        let lookahead = input.lookahead1();
        if lookahead.peek(token::opaque) {
            input.parse::<token::opaque>().map(FfiStructKind::Opaque)
        } else if lookahead.peek(token::shared) {
            input.parse::<token::shared>().map(FfiStructKind::Shared)
        } else {
            Err(lookahead.error())
        }
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
enum FfiStructParsedData {
    Opaque { inner_type: Type },
    Shared,
}

pub(crate) fn ffi_struct_internal(
    kind: FfiStructKind,
    input_struct: ItemStruct,
) -> SynResult<TokenStream> {
    let parsed_data = match kind {
        FfiStructKind::Opaque(_) => verify_struct_opaque(&input_struct)?,
        FfiStructKind::Shared(_) => verify_struct_shared(&input_struct)?,
    };

    let mut stream = TokenStream::new();

    create_ffi_struct(&parsed_data, &input_struct).to_tokens(&mut stream);
    create_stub_struct(&parsed_data, &input_struct).to_tokens(&mut stream);

    Ok(stream)
}

fn create_ffi_struct(kind: &FfiStructParsedData, input_struct: &ItemStruct) -> impl ToTokens {
    match kind {
        FfiStructParsedData::Opaque { ref inner_type } => {
            let struct_name = input_struct.ident.clone();

            quote! {
                #[cfg(not(comms_stack_stub))]
                #[repr(transparent)]
                #input_struct

                #[cfg(not(comms_stack_stub))]
                // SAFETY: The struct is annotated with #[repr(transparent)] and is
                // guaranteed to be a plain data structure with no interior pointers or references.
                unsafe impl bytemuck::TransparentWrapper<#inner_type> for #struct_name {}
            }
        }
        FfiStructParsedData::Shared => {
            quote! {
                #[cfg(not(comms_stack_stub))]
                #input_struct
            }
        }
    }
}

fn create_stub_struct(kind: &FfiStructParsedData, input_struct: &ItemStruct) -> impl ToTokens {
    let mut stub_struct = input_struct.clone();

    let attr_cfg: Attribute = parse_quote! { #[cfg(comms_stack_stub)] };
    stub_struct.attrs.push(attr_cfg);
    let attr_missing_docs: Attribute = parse_quote! { #[allow(missing_docs)] };
    stub_struct.attrs.push(attr_missing_docs);
    match kind {
        FfiStructParsedData::Opaque { .. } => {
            stub_struct.fields = Fields::Unit;
        }
        FfiStructParsedData::Shared => {}
    }

    stub_struct
}

/// Validates the annotated opaque struct to ensure that the signature matches expected FFI style.
fn verify_struct_opaque(input_struct: &ItemStruct) -> SynResult<FfiStructParsedData> {
    fn check_attr_is_repr(attr: &Attribute) -> bool {
        match &attr.meta {
            Meta::Path(path) => path.is_ident("repr"),
            Meta::List(list) => list.path.is_ident("repr"),
            _ => false,
        }
    }
    if input_struct.attrs.iter().any(check_attr_is_repr) {
        let msg =
            format!("Structs with {OPAQUE_STRUCT_MACRO} MUST NOT be declared with #[repr(...)].");
        return Err(SynError::new(input_struct.span(), msg));
    }

    let punctuated_fields = match &input_struct.fields {
        Fields::Named(fields) => &fields.named,
        Fields::Unnamed(fields) => &fields.unnamed,
        Fields::Unit => {
            let msg = format!("Structs with {OPAQUE_STRUCT_MACRO} MUST declare exactly one field.");
            return Err(SynError::new(input_struct.span(), msg));
        }
    };
    if punctuated_fields.len() != 1 {
        let msg = format!("Structs with {OPAQUE_STRUCT_MACRO} MUST declare exactly one field.");
        return Err(SynError::new(input_struct.span(), msg));
    }
    let inner_type = punctuated_fields[0].ty.clone();

    Ok(FfiStructParsedData::Opaque { inner_type })
}

/// Validates the annotated shared struct to ensure that the signature matches expected FFI style.
fn verify_struct_shared(input_struct: &ItemStruct) -> SynResult<FfiStructParsedData> {
    let attr_repr_c: Attribute = parse_quote! { #[repr(C)] };
    if !input_struct.attrs.contains(&attr_repr_c) {
        let msg = format!("Structs with {SHARED_STRUCT_MACRO} MUST be declared as #[repr(C)].");
        return Err(SynError::new(input_struct.span(), msg));
    }

    Ok(FfiStructParsedData::Shared)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opaque() {
        let opaque: FfiStructKind = parse_quote! { opaque };
        let test_struct: ItemStruct = parse_quote! {
            pub struct TestStruct {
                val: u32,
            }
        };

        let res = ffi_struct_internal(opaque, test_struct).unwrap();

        let expected = quote! {
            #[cfg(not(comms_stack_stub))]
            #[repr(transparent)]
            pub struct TestStruct {
                val: u32,
            }

            #[cfg(not(comms_stack_stub))]
            // SAFETY: The struct is annotated with #[repr(transparent)] and is
            // guaranteed to be a plain data structure with no interior pointers or references.
            unsafe impl bytemuck::TransparentWrapper<u32> for TestStruct {}

            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub struct TestStruct;
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_shared() {
        let shared: FfiStructKind = parse_quote! { shared };
        let test_struct: ItemStruct = parse_quote! {
            #[repr(C)]
            pub struct TestStruct {
                val: u32,
            }
        };

        let res = ffi_struct_internal(shared, test_struct).unwrap();

        let expected = quote! {
            #[cfg(not(comms_stack_stub))]
            #[repr(C)]
            pub struct TestStruct {
                val: u32,
            }

            #[repr(C)]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub struct TestStruct {
                val: u32,
            }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }
}
