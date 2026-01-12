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

//! Macros for managing SDV Comms FFI.
//!
//! There are two macros provided by this crate: `comms_stack_ffi_fn` and `comms_stack_ffi_struct`
//!
//! These macros generate code that can be controlled by the following exclusive configurations:
//!  * cfg(comms_stack_ffi): Code that should ONLY be included in the ffi version of the library.
//!  * cfg(comms_stack_stub): Code that should ONLY be included in the stub version of the library.
//!  * Everything else: Code that should be included in all versions of the library.

mod ffi_fn;
mod ffi_struct;

use proc_macro::TokenStream;
use syn::{parse_macro_input, ItemFn, ItemStruct};

use crate::ffi_struct::FfiStructKind;

/// Attribute macro that emits ffi and stub versions of annotated functions.
///
/// Example usage:
/// ```
///   #[comms_stack_ffi_fn]
///   #[no_mangle]
///   pub unsafe extern "C" fn ...(....) { ... }
/// ```
/// the above macro expand the annotated function into the following:
/// ```
///   #[cfg(comms_stack_ffi)]
///   #[no_mangle]
///   pub unsafe extern "C" fn ...(....) { ... }
///
///   #[cfg(comms_stack_stub)]
///   #[no_mangle]
///   pub unsafe extern "C" fn ...(....) { unimplemented!() }
/// ```
#[proc_macro_attribute]
pub fn comms_stack_ffi_fn(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);

    ffi_fn::ffi_fn_internal(input_fn).unwrap_or_else(syn::Error::into_compile_error).into()
}

/// Attribute macro that emits ffi and stub versions of annotated struct declarations.
///
/// Example usage:
/// ```
///   #[comms_stack_ffi_struct(opaque)]
///   pub struct ExampleOpaqueStruct(ImplType)
///   #[cfg(not(comms_stack_stub))]
///   // SAFETY: The struct is annotated with #[repr(transparent)] and is
///   // guaranteed to be a plain data structure with no interior pointers or references.
///   unsafe impl bytemuck::TransparentWrapper<ImplType> for ExampleOpaqueStruct {}
///
///   #[comms_stack_ffi_struct(shared)]
///   #[repr(C)]
///   pub struct ExampleSharedStruct {
///       val: u32,
///   }
/// ```
/// the above macro expands the annotated structs into the following:
/// ```
///   #[cfg(not(comms_stack_stub))]
///   #[repr(transparent)]
///   pub struct ExampleOpaqueStruct(ImplType)
///   #[cfg(comms_stack_stub)]
///   pub struct ExampleOpaqueStruct();
///   // SAFETY: The struct is annotated with #[repr(transparent)] and is
///   // guaranteed to be a plain data structure with no interior pointers or references.
///   unsafe impl bytemuck::TransparentWrapper<ImplType> for ExampleOpaqueStruct {}
///
///   #[cfg(not(comms_stack_stub))]
///   #[repr(C)]
///   pub struct ExampleSharedStruct {
///       val: u32,
///   }
///   #[cfg(comms_stack_stub)]
///   #[repr(C)]
///   pub struct ExampleSharedStruct {
///       val: u32,
///   }
/// ```
#[proc_macro_attribute]
pub fn comms_stack_ffi_struct(attr: TokenStream, input: TokenStream) -> TokenStream {
    let struct_kind = parse_macro_input!(attr as FfiStructKind);
    let input_struct = parse_macro_input!(input as ItemStruct);

    ffi_struct::ffi_struct_internal(struct_kind, input_struct)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}
