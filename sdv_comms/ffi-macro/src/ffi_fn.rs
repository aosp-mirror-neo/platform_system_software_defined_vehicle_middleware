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

//! Implementation of the #[comms_stack_ffi_fn] macro.

use proc_macro2::TokenStream;
use quote::{format_ident, ToTokens};
use syn::parse::{Error as SynError, Result as SynResult};
use syn::spanned::Spanned;
use syn::{
    parse_quote, Attribute, Block, Expr, FnArg, ItemFn, Meta, PatType, Path, ReturnType, Stmt, Type,
};

const FN_MACRO: &str = "#[comms_stack_ffi_fn]";

const ATTR_ALLOW_VALUE: &str = "allow_value";
const ATTR_ALLOW_PTR: &str = "allow_ptr";
const ATTR_ALLOW_REFERENCE: &str = "allow_reference";

pub(crate) fn ffi_fn_internal(mut input_fn: ItemFn) -> SynResult<TokenStream> {
    verify_fn(&input_fn)?;

    let stmts = process_args(&mut input_fn)?;
    let out_stmt = process_output(&mut input_fn)?;

    let mut stream = TokenStream::new();

    create_ffi_fn(&input_fn, stmts, out_stmt).to_tokens(&mut stream);
    create_stub_fn(&input_fn).to_tokens(&mut stream);

    Ok(stream)
}

const BOX_AS_PATH: &[&str] = &["std", "boxed", "Box"];
const OPTION_AS_PATH: &[&str] = &["std", "option", "Option"];
const SDV_STATUS_AS_PATH: &[&str] = &["sdv_status_rs", "sdv_status_t"];

const FFI_ALLOWED_PRIMITIVES: &[&str] =
    &["u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "f32", "f64", "bool", "usize"];

/// Updates the input arguments from the provided `input_fn` and returns a set of initialisation
/// [Stmt]s for the updated arguments.
///
/// This function calls [map_input_args] to perform the conversion. See docs for what conversion
/// types are supported.
fn process_args(input_fn: &mut ItemFn) -> SynResult<Vec<Stmt>> {
    let arg_stmts: Vec<Option<Stmt>> = input_fn.sig.inputs.iter_mut()
        .map(|arg|
            match arg {
                FnArg::Receiver(_) => {
                    let msg = format!(
                        "Functions with {FN_MACRO} MUST NOT have `self`, `&self`, `&mut self` arguments.");
                    Err(SynError::new(arg.span(), msg))
                },
                FnArg::Typed(pat_type) => {
                    let (new_arg, opt_stmt) = map_input_arg(pat_type)?;
                    *pat_type = new_arg;
                    Ok(opt_stmt)
                },
            }
        )
        .collect::<Result<Vec<Option<Stmt>>, _>>()?;

    Ok(arg_stmts.into_iter().flatten().collect())
}

/// Updates the return type for the provided `input_fn` and returns a set of initialisation
/// [Stmt]s for the updated return type.
///
/// This function calls [map_output_type] to perform the conversion. See docs for what conversion
/// types are supported.
fn process_output(input_fn: &mut ItemFn) -> SynResult<Option<Stmt>> {
    let attr_allow_ref: Attribute = parse_quote! { #[allow_reference] };
    let allow_ref = input_fn.attrs.contains(&attr_allow_ref);
    input_fn.attrs.retain(|a| !check_attr_exact(a, ATTR_ALLOW_REFERENCE));

    if let ReturnType::Type(_, box_type) = &mut input_fn.sig.output {
        let (new_ret, out_stmt) = map_output_type(box_type, allow_ref)?;
        *box_type = Box::new(new_ret);
        return Ok(out_stmt);
    }
    Ok(None)
}

/// Converts arguments passed to the function into raw pointer types.
///
/// Performs the following conversions:
///   Box<T>         --> *mut T
///   Option<Box<T>> --> *mut T
///   &T             --> *const T
///   &mut T         --> *mut T
///
/// # Return Value
/// Returns the tuple `(Type, Option<Stmt>)`. The returned tuple contains the [Type] for the
/// replacement of `arg_with_type` and an optional [Stmt] to be inserted in the beginning of the
/// function to perform the conversion from the new arg type to the expected type.
fn map_input_arg(arg_with_type: &PatType) -> SynResult<(PatType, Option<Stmt>)> {
    let name = &arg_with_type.pat;
    let mut cloned_arg = arg_with_type.clone();
    match arg_with_type.ty.as_ref() {
        Type::Path(type_path) => {
            if type_path.qself.is_some() {
                let msg =
                    format!("Functions with {FN_MACRO} MUST NOT have Quoted Self type arguments.");
                return Err(SynError::new(arg_with_type.span(), msg));
            }

            cloned_arg.attrs.retain(|a| !check_attr_exact(a, ATTR_ALLOW_VALUE));
            // Check if the attribute `#[allow_ptr]` was used.
            if cloned_arg.attrs.len() != arg_with_type.attrs.len() {
                return Ok((cloned_arg, None));
            }

            // Check if the argument type is an FFI safe primitive type.
            if type_path.path.segments.len() == 1 {
                let first_segment = type_path.path.segments.first().unwrap();
                if FFI_ALLOWED_PRIMITIVES.iter().any(|prim_ty| first_segment.ident == prim_ty) {
                    return Ok((cloned_arg, None));
                }
            }

            // Check if the argument type is `Box<T>`; then convert it into `*mut T`.
            if let Some(boxed_type) = extract_generic_from_type(&type_path.path, BOX_AS_PATH) {
                let changed_type: Type = parse_quote!( *mut #boxed_type );
                let cast_stmt: Stmt = parse_quote! {
                    let #name: #type_path = if #name.is_null() {
                        return ::sdv_status_rs::SdvStatusCode::InvalidArgument.into_raw();
                    } else {
                        ::std::boxed::Box::from_raw(#name)
                    };
                };
                cloned_arg.ty = Box::new(changed_type);
                return Ok((cloned_arg, Some(cast_stmt)));
            }

            // Check if the argument type is ::std::boxed::Option<T>.
            if let Some(option_type) = extract_generic_from_type(&type_path.path, OPTION_AS_PATH) {
                // We will only handle the following cases:
                //   `Option<Box<T>>`, `Option<&T>`, and `Option<&mut T>`.
                // Anything else is an error.
                if let Type::Path(ref option_path_type) = option_type {
                    if let Some(option_boxed_type) =
                        extract_generic_from_type(&option_path_type.path, BOX_AS_PATH)
                    {
                        // The argument type is `Option<Box<T>>`; extract it and perform safe casting.
                        let changed_type: Type = parse_quote!( *mut #option_boxed_type );
                        let cast_stmt: Stmt = parse_quote! {
                            let #name: #type_path = if #name.is_null() {
                                ::std::option::Option::None
                            } else {
                                ::std::option::Option::Some(::std::boxed::Box::from_raw(#name))
                            };
                        };
                        cloned_arg.ty = Box::new(changed_type);
                        return Ok((cloned_arg, Some(cast_stmt)));
                    }
                }

                if let Type::Reference(_) = option_type {
                    // Rust will safely convert Option<&T> and Option<&mut T> values from pointers, so we do not need to do anything here.
                    return Ok((cloned_arg, None));
                }
            }

            let suggestion = {
                let mut suggestion = arg_with_type.clone();
                let allow_value_token = format_ident!("{}", ATTR_ALLOW_VALUE);
                suggestion.attrs.push(parse_quote! { #[#allow_value_token] });
                suggestion.into_token_stream()
            };
            let msg = format!(
                "Functions with {FN_MACRO} MUST NOT have owned values in arguments except `Box<T>`\
                 , `Option<&T>`, Option<&mut T> and `Option<Box<T>>` types.\nIf you are sure \
                 this should be passed by value, add the annotation `#[{ATTR_ALLOW_VALUE}]` to \
                 the argument. Example:\n    {suggestion}"
            );
            Err(SynError::new(arg_with_type.span(), msg))
        }
        Type::Ptr(_) => {
            cloned_arg.attrs.retain(|a| !check_attr_exact(a, ATTR_ALLOW_PTR));
            // Check if the attribute `#[allow_ptr]` was used.
            if cloned_arg.attrs.len() != arg_with_type.attrs.len() {
                return Ok((cloned_arg, None));
            }

            let suggestion = {
                let mut suggestion = arg_with_type.clone();
                let allow_ptr_token = format_ident!("{}", ATTR_ALLOW_PTR);
                suggestion.attrs.push(parse_quote! { #[#allow_ptr_token] });
                suggestion.into_token_stream()
            };
            let msg = format!(
                "Functions with {FN_MACRO} MUST NOT have pointer arguments.\nIf you are sure \
                 this should be passed by pointer, add the annotation `#[{ATTR_ALLOW_PTR}]` to \
                 the argument. Example:\n    {suggestion}"
            );
            Err(SynError::new(arg_with_type.span(), msg))
        }
        Type::Reference(ty_ref) => {
            // The provided argument type is either `&T` or `&mut T`; convert it into `*const T`
            // and `*mut T` respectively.
            let ref_elem = &ty_ref.elem;
            let (changed_type, cast_expr) = if ty_ref.mutability.is_some() {
                let changed_type: Type = parse_quote! { *mut #ref_elem };
                let cast_expr: Expr = parse_quote! { #name.as_mut() };
                (changed_type, cast_expr)
            } else {
                let changed_type: Type = parse_quote! { *const #ref_elem };
                let cast_expr: Expr = parse_quote! { #name.as_ref() };
                (changed_type, cast_expr)
            };

            let cast_stmt: Stmt = parse_quote! {
                let #name: #ty_ref = match #cast_expr {
                    Some(reference) => reference,
                    None => return ::sdv_status_rs::SdvStatusCode::InvalidArgument.into_raw(),
                };
            };

            cloned_arg.ty = Box::new(changed_type);
            Ok((cloned_arg, Some(cast_stmt)))
        }
        _ => {
            let msg = format!("Functions with {FN_MACRO} has an argument of unsupported kind");
            Err(SynError::new(arg_with_type.span(), msg))
        }
    }
}

/// Converts out arguments passed to the function into raw pointer types.
///
/// Performs the following conversions:
///   Box<T>         --> *mut T
///   Option<Box<T>> --> *mut T
///
/// # Return Value
/// Returns the tuple `(Type, Option<Stmt>)`. The returned tuple contains the [Type] for the
/// replacement of `arg_with_type` and an optional [Stmt] to be inserted in the beginning of the
/// function to perform the conversion from the new arg type to the expected type.
fn map_output_type(output_type: &Type, allow_ref: bool) -> SynResult<(Type, Option<Stmt>)> {
    match output_type {
        Type::Path(type_path) => {
            if type_path.qself.is_some() {
                let msg = format!(
                    "Functions with {FN_MACRO} MUST NOT have Quoted Self type return types."
                );
                return Err(SynError::new(output_type.span(), msg));
            }

            // Check if the return type is an FFI safe primitive type.
            if type_path.path.segments.len() == 1 {
                let first_segment = type_path.path.segments.first().unwrap();
                if FFI_ALLOWED_PRIMITIVES.iter().any(|prim_ty| first_segment.ident == prim_ty) {
                    return Ok((output_type.clone(), None));
                }
            }
            // Check if the return type is sdv_status_t
            if matches_type_path(&type_path.path, SDV_STATUS_AS_PATH) {
                return Ok((output_type.clone(), None));
            }

            // Check if the return type is `Box<T>`; then convert it into `*mut T`.
            if let Some(boxed_type) = extract_generic_from_type(&type_path.path, BOX_AS_PATH) {
                let changed_type: Type = parse_quote!( *mut #boxed_type );
                let return_stmt: Stmt = parse_quote! {
                    return ::std::boxed::Box::into_raw(output);
                };
                return Ok((changed_type, Some(return_stmt)));
            }

            // Check if the return type is Option<Box<T>>
            if let Some(Type::Path(ref option_path_type)) =
                extract_generic_from_type(&type_path.path, OPTION_AS_PATH)
            {
                if let Some(option_boxed_type) =
                    extract_generic_from_type(&option_path_type.path, BOX_AS_PATH)
                {
                    // The argument type is `Option<Box<T>>`; extract it and perform safe casting.
                    let changed_type: Type = parse_quote!( *mut #option_boxed_type );
                    let return_stmt: Stmt = parse_quote! {
                        return output.map(|boxed_value| ::std::boxed::Box::into_raw(boxed_value)).unwrap_or(::std::ptr::null_mut());
                    };
                    return Ok((changed_type, Some(return_stmt)));
                }
            }

            let msg = format!(
                "Functions with {FN_MACRO} MUST NOT have owned values as return types except `Box<T>`\
                 and `Option<Box<T>>` types.\n"
            );
            Err(SynError::new(output_type.span(), msg))
        }

        Type::Reference(_) => {
            // Check if the attribute `#[allow_reference]` was used.
            if allow_ref {
                return Ok((output_type.clone(), None));
            }

            let msg = format!(
                "Functions with {FN_MACRO} MUST NOT have reference return types.\nIf you are sure \
                 this function should return a reference, add the annotation `#[{ATTR_ALLOW_REFERENCE}]` to \
                 the argument."
            );
            Err(SynError::new(output_type.span(), msg))
        }
        _ => {
            let msg = format!("Functions with {FN_MACRO} has a return type of unsupported kind");
            Err(SynError::new(output_type.span(), msg))
        }
    }
}

/// Checks if the provided attribute is exactly `#[{expected}]`.
fn check_attr_exact(attr: &Attribute, expected: &str) -> bool {
    match &attr.meta {
        Meta::Path(path) => path.is_ident(expected),
        Meta::List(list) => list.path.is_ident(expected),
        _ => false,
    }
}

/// Matches the provided `path` against `name` and extracts the first generic args it.
///
/// NOTE: This function only expects and extracts the first generic argument.
/// NOTE: `name` must be fully qualified name of the type to match.
///
/// Examples:
///   extract_generic_from_type(
///       parse_quote! { std::boxed::Box<MyType> },
///       vec!["std", "boxed", "Box"]
///   ) returns Some(`MyType`)
///
///   extract_generic_from_type(
///       parse_quote! { Box<MyType> },
///       vec!["std", "boxed", "Box"]
///   ) returns Some(`MyType`)
///
///   extract_generic_from_type(
///       parse_quote! { Vec<'a, MyType> },
///       vec!["std", "vec", "Vec"]
///   ) returns None
///
///   extract_generic_from_type(
///       parse_quote! { HashMap<Key, Value> },
///       vec!["std", "collections", "HashMap"]
///   ) returns None
fn extract_generic_from_type(path: &Path, name: &[&str]) -> Option<Type> {
    assert!(!name.is_empty());

    if !matches_type_path(path, name) {
        return None;
    }

    let segment = path.segments.last().unwrap();
    let first_generic_args = match &segment.arguments {
        syn::PathArguments::AngleBracketed(args) if args.args.len() == 1 => {
            args.args.first().unwrap()
        }
        _ => {
            return None;
        }
    };

    match first_generic_args {
        syn::GenericArgument::Type(ty) => Some(ty.clone()),
        _ => None,
    }
}

/// Matches the provided `path` against `name`.
///
/// NOTE: `name` must be fully qualified name of the type to match.
///
/// Examples:
///   matches_type_path(
///       parse_quote! { std::boxed::Box<MyType> },
///       vec!["std", "boxed", "Box"]
///   ) returns true
///
///   matches_type_path(
///       parse_quote! { Box<MyType> },
///       vec!["std", "boxed", "Box"]
///   ) returns true
///
///   matches_type_path(
///       parse_quote! { std::Iter },
///       vec!["std", "vec", "Vec"]
///   ) returns false
///
///   matches_type_path(
///       parse_quote! { sdv_status_t },
///       vec!["sdv_status_rs", "sdv_status_t"]
///   ) returns true
fn matches_type_path(path: &Path, name: &[&str]) -> bool {
    assert!(!name.is_empty());

    if path.segments.len() == name.len() {
        if (path.segments.iter())
            .zip(name.iter())
            .all(|(path_elem, name_elem)| path_elem.ident == name_elem)
        {
            return true;
        }
    } else if path.segments.len() == 1 {
        let first_segment = path.segments.first().unwrap();
        if first_segment.ident == name.last().unwrap() {
            return true;
        }
    }

    false
}

/// Annotates the actual function with `#[cfg(comms_stack_ffi)]`.
fn create_ffi_fn(input_fn: &ItemFn, init_stmts: Vec<Stmt>, out_stmt: Option<Stmt>) -> ItemFn {
    let mut ffi_fn = input_fn.clone();

    let attr_cfg: Attribute = parse_quote! { #[cfg(comms_stack_ffi)] };
    ffi_fn.attrs.push(attr_cfg);

    if !init_stmts.is_empty() {
        let existing_block = &ffi_fn.block;
        ffi_fn.block = parse_quote! {
            {
                #(#init_stmts)*
                #existing_block
            }
        };
    }

    if let Some(stmt) = out_stmt {
        let existing_block = &ffi_fn.block;
        ffi_fn.block = parse_quote! {
            {
                let output = #existing_block;
                #stmt
            }
        };
    }

    ffi_fn
}

/// Creates a stub function and annotates it with `#[cfg(comms_stack_stub)]`.
fn create_stub_fn(input_fn: &ItemFn) -> ItemFn {
    let mut stub_fn = input_fn.clone();

    let attr_cfg: Attribute = parse_quote! { #[cfg(comms_stack_stub)] };
    stub_fn.attrs.push(attr_cfg);
    let attr_missing_docs: Attribute = parse_quote! { #[allow(missing_docs)] };
    stub_fn.attrs.push(attr_missing_docs);

    let stub_fn_body: Block = parse_quote! {
        { unimplemented!() }
    };
    stub_fn.block = Box::new(stub_fn_body);

    stub_fn
}

/// Validates the annotated function to ensure that the signature matches expected FFI style.
fn verify_fn(input_fn: &ItemFn) -> SynResult<()> {
    let attr_no_mangle: Attribute = parse_quote! { #[no_mangle] };
    if !input_fn.attrs.contains(&attr_no_mangle) {
        let msg = format!("Functions with {FN_MACRO} MUST be declared with attribute #[no_mangle]");
        return Err(SynError::new(input_fn.span(), msg));
    }

    if !matches!(input_fn.vis, syn::Visibility::Public(_)) {
        let msg = format!("Functions with {FN_MACRO} MUST be declared public");
        return Err(SynError::new(input_fn.span(), msg));
    }

    if input_fn.sig.constness.is_some() {
        let msg = format!("Functions with {FN_MACRO} MUST NOT be declared `const`");
        return Err(SynError::new(input_fn.span(), msg));
    }

    if input_fn.sig.constness.is_some() {
        let msg = format!("Functions with {FN_MACRO} MUST NOT be declared `const`");
        return Err(SynError::new(input_fn.span(), msg));
    }

    if input_fn.sig.asyncness.is_some() {
        let msg = format!("Functions with {FN_MACRO} MUST NOT be declared `async`");
        return Err(SynError::new(input_fn.span(), msg));
    }

    if input_fn.sig.unsafety.is_none() {
        let msg = format!("Functions with {FN_MACRO} MUST be declared `unsafe`");
        return Err(SynError::new(input_fn.span(), msg));
    }

    let has_abi = input_fn
        .sig
        .abi
        .clone()
        .and_then(|abi| abi.name)
        .map(|name| name.value() == "C")
        .unwrap_or(false);
    if !has_abi {
        let msg = format!("Functions with {FN_MACRO} MUST be declared `extern \"C\"`");
        return Err(SynError::new(input_fn.span(), msg));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use quote::quote;

    #[test]
    fn test_simple() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function() {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function() {
                println!("Hello World");
            }

            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function() { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_with_primitive_type_arg() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function(
                    arg_u8: u8, arg_u16: u16, arg_u32: u32, arg_u64: u64,
                    arg_i8: i8, arg_i16: i16, arg_i32: i32, arg_i64: i64,
                    arg_f32: f32, arg_f64: f64, arg_bool: bool) {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function(
                    arg_u8: u8, arg_u16: u16, arg_u32: u32, arg_u64: u64,
                    arg_i8: i8, arg_i16: i16, arg_i32: i32, arg_i64: i64,
                    arg_f32: f32, arg_f64: f64, arg_bool: bool) {
                println!("Hello World");
            }

            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function(
                    arg_u8: u8, arg_u16: u16, arg_u32: u32, arg_u64: u64,
                    arg_i8: i8, arg_i16: i16, arg_i32: i32, arg_i64: i64,
                    arg_f32: f32, arg_f64: f64, arg_bool: bool) {
                unimplemented!()
            }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_with_box_arg() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function(arg1: Box<Type1>) {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function(arg1: *mut Type1) {
                let arg1: Box<Type1> = if arg1.is_null() {
                    return ::sdv_status_rs::SdvStatusCode::InvalidArgument.into_raw();
                } else {
                    ::std::boxed::Box::from_raw(arg1)
                };
                {
                    println!("Hello World");
                }
            }

            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function(arg1: *mut Type1) { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_with_unknown_type_arg() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function(arg: SomeType) {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn);

        assert!(res.is_err());
    }

    #[test]
    fn test_fn_with_const_ref_arg() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function(arg1: &'lifetime Type1) {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function(arg1: *const Type1) {
                let arg1: &'lifetime Type1 = match arg1.as_ref() {
                    Some(reference) => reference,
                    None => return ::sdv_status_rs::SdvStatusCode::InvalidArgument.into_raw(),
                };
                {
                    println!("Hello World");
                }
            }

            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function(arg1: *const Type1) { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_with_mut_ref_arg() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function(arg1: &'lifetime mut Type1) {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function(arg1: *mut Type1) {
                let arg1: &'lifetime mut Type1 = match arg1.as_mut() {
                    Some(reference) => reference,
                    None => return ::sdv_status_rs::SdvStatusCode::InvalidArgument.into_raw(),
                };
                {
                    println!("Hello World");
                }
            }

            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function(arg1: *mut Type1) { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_with_option_box_arg() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function(arg1: Option<Box<Type1>>) {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function(arg1: *mut Type1) {
                let arg1: Option< Box<Type1> > = if arg1.is_null() {
                    ::std::option::Option::None
                } else {
                    ::std::option::Option::Some(::std::boxed::Box::from_raw(arg1))
                };
                {
                    println!("Hello World");
                }
            }

            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function(arg1: *mut Type1) { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_with_option_const_ref_arg() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function(arg1: Option<&Type1>) {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function(arg1: Option<&Type1>) {
                println!("Hello World");
            }

            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function(arg1: Option<&Type1>) { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_with_option_mut_ref_arg() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function(arg1: Option<&mut Type1>) {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function(arg1: Option<&mut Type1>) {
                println!("Hello World");
            }

            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function(arg1: Option<&mut Type1>) { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_with_const_pointer_fails() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function(arg1: *const Type1) {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn);

        assert!(res.is_err());
    }

    #[test]
    fn test_fn_with_mut_pointer_fails() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function(arg1: *mut Type1) {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn);

        assert!(res.is_err());
    }

    #[test]
    fn test_fn_with_const_pointer_allow_annotated() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function(#[allow_ptr] arg1: *const Type1) {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function(arg1: *const Type1) {
                println!("Hello World");
            }

            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function(arg1: *const Type1) { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_with_mut_pointer_allow_annotated() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function(#[allow_ptr] arg1: *mut Type1) {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function(arg1: *mut Type1) {
                println!("Hello World");
            }

            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function(arg1: *mut Type1) { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_primitive_ret_type() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function() -> bool {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function() -> bool {
                println!("Hello World");
            }


            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function() -> bool { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_sdv_status_ret_type() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function() -> sdv_status_t {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function() -> sdv_status_t {
                println!("Hello World");
            }


            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function() -> sdv_status_t { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_box_ret_type() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function() -> Box<T> {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function() -> *mut T {
                let output = {
                    println!("Hello World");
                };
                return ::std::boxed::Box::into_raw(output);
            }


            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function() -> *mut T { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_option_box_ret_type() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function() -> Option<Box<T>> {
                println!("Hello World");
                T
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function() -> *mut T {
                let output = {
                    println!("Hello World");
                    T
                };
                return output.map(|boxed_value| ::std::boxed::Box::into_raw(boxed_value)).unwrap_or(::std::ptr::null_mut());
            }


            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function() -> *mut T { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_option_box_arg_and_ret_type() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            pub unsafe extern "C" fn test_function(arg: Option<Box<T>>) -> Option<Box<T>> {
                println!("Hello World");
                T
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function(arg: *mut T) -> *mut T {
                let output = {
                    let arg: Option< Box<T> > = if arg.is_null() {
                        ::std::option::Option::None
                    } else {
                        ::std::option::Option::Some(::std::boxed::Box::from_raw(arg))
                    };
                    {
                        println!("Hello World");
                        T
                    }
                };
                return output.map(|boxed_value| ::std::boxed::Box::into_raw(boxed_value)).unwrap_or(::std::ptr::null_mut());
            }


            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function(arg: *mut T) -> *mut T { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }

    #[test]
    fn test_fn_reference_ret_type() {
        let test_fn: ItemFn = parse_quote! {
            #[no_mangle]
            #[allow_reference]
            pub unsafe extern "C" fn test_function() -> &T {
                println!("Hello World");
            }
        };

        let res = ffi_fn_internal(test_fn).unwrap();

        let expected = quote! {
            #[no_mangle]
            #[cfg(comms_stack_ffi)]
            pub unsafe extern "C" fn test_function() -> &T {
                println!("Hello World");
            }

            #[no_mangle]
            #[cfg(comms_stack_stub)]
            #[allow(missing_docs)]
            pub unsafe extern "C" fn test_function() -> &T { unimplemented!() }
        };
        assert_eq!(res.to_string(), expected.to_string());
    }
}
