// Copyright (C) 2026 The Android Open Source Project
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Utilities for custom certificate verifiers.

use crate::mesh::{
    credentials_checker::CredentialsChecker, sd_certificate_claims::SdCertificateClaims,
};
use anyhow::{anyhow, bail, Context};
use grpcio::{ChannelCredentials, ServerCredentials};
use grpcio_sys::grpc_status_code;
use log::{debug, error, info};
use openssl::{
    pkey::{PKey, Private},
    x509::X509,
};
use std::{
    ffi::{CStr, CString},
    marker::PhantomPinned,
    mem::ManuallyDrop,
    os::raw::{c_char, c_int, c_void},
    pin::Pin,
    ptr::NonNull,
    sync::Mutex,
};

/// Custom x509 certificate verifier.
///
/// # Safety
///
/// - `user_data` must point to an `ExternalVerifier` object created with ExternalVerifier::new
///   that is not dropped or modified during the execution of this function.
/// - `sync_status` must either be null or non-null and valid for writes, must not be accessed during the execution of this function
/// - `request` must either be null or non-null and valid during the execution of this function
/// - `request.target_name` must be non-null nul-terminated C string.
/// - `request.peer_info.peer_cert` must be non-null nul-terminated C string.
/// - `request.peer_info.common_name` must be a nul-terminated C string.
/// - `request.peer_info.san_names.dns_names` and `request.peer_info.san_names.dns_names_size` must satisfy the safety requirements of `null_safe_from_raw_parts`.
/// - `request.peer_info.san_names.ip_names` and `request.peer_info.san_names.ip_names_size` must satisfy the safety requirements of `null_safe_from_raw_parts`.
///
/// Note: this function is meant to be passed to gRPC as a callback. The safety requirements listed above rather express our expectation of gRPC.
pub unsafe extern "C" fn custom_verifier(
    user_data: *mut c_void,
    request: *mut grpcio_sys::grpc_tls_custom_verification_check_request,
    _callback: grpcio_sys::grpc_tls_on_custom_verification_check_done_cb,
    _callback_arg: *mut c_void,
    sync_status: *mut grpc_status_code::Type,
    sync_error_details: *mut *mut c_char,
) -> c_int {
    info!("Custom TLS certificate verifier invoked");
    if sync_error_details.is_null() || sync_status.is_null() {
        return 1;
    }

    // SAFETY: `sync_status` pointer is valid for write and is promised to not be accessed during
    // the execution of this function. `sync_error_details` and `sync_status` are null-checked
    // above. We're allocating a single zero byte corresponding to an empty error message. It's
    // necessary to set the error message when `GRPC_STATUS_UNAUTHENTICATED` is returned. The
    // caller is responsible for deallocating it.
    unsafe {
        *sync_error_details = grpcio_sys::gpr_zalloc(1) as *mut c_char;
        *sync_status = grpc_status_code::GRPC_STATUS_UNAUTHENTICATED;
    }

    if request.is_null() {
        error!("request not found");
        return 1;
    }
    // SAFETY: `request` is null-checked above, points to a valid object that is read-available
    // during the execution of this function.
    let request = &unsafe { *request };

    if user_data.is_null() {
        error!("user_data not found");
        return 1;
    }

    let target_name = match extract_target_name(request) {
        Ok(target_name) => target_name,
        Err(err) => {
            error!("{err}");
            return 1;
        }
    };

    // SAFETY: `user_data` points to a `ExternalVerifier` instance owned by the Rust side
    // (ie, the client side of the FFI) and is set set in its `new`.
    // It's also null-checked above.
    let external_verifier: &ExternalVerifier = unsafe { &*(user_data as *mut ExternalVerifier) };

    if request.peer_info.peer_cert.is_null() {
        error!("peer_cert not found");
        return 1;
    }

    // SAFETY:
    // - `request->peer_info.peer_cert` is a nul-terminated C string.
    let peer_cert_cstr = unsafe { CStr::from_ptr(request.peer_info.peer_cert) };
    match X509::from_pem(peer_cert_cstr.to_bytes())
        .map_err(|err| anyhow!(err))
        .and_then(|peer_leaf_cert| SdCertificateClaims::from_x509(&peer_leaf_cert))
        .and_then(|sd_cert_claims| {
            check_claims_match_target(sd_cert_claims, target_name.as_deref())
        })
        .and_then(|sd_cert_claims| external_verifier.credentials_checker.check(&sd_cert_claims))
    {
        // SAFETY: `sync_status` is null-checked at the beginning of this function. As `sync_status` is non-null, the caller promises it to point to a valid-for-write object.
        Ok(()) => unsafe { *sync_status = grpc_status_code::GRPC_STATUS_OK },
        Err(err) => error!("Failed: {err:#}"),
    }

    1 // performing the check synchronously
}

/// Called from the other side of the FFI by grpcio's C++ implementation to notify
/// the FFI client side (us, the Rust code) that grpcio no longer needs the external verifier
/// given to it and it must thus be destroyed to avoid a memory leak.
///
/// SAFETY: If `user_data` is non-null, it must point to the raw address of a
/// Pin<Box<ExternalVerifier>> that was created with ExternalVerifier::new() but whose memory is no
/// longer managed by Rust. Ie, it was Pin::into_inner_unchecked() and Box::into_raw(). As this
/// function is now the sole user of this `user_data` memory, it's safe to reclaim and drop it.
pub unsafe extern "C" fn destruct_external_verifier(user_data: *mut c_void) {
    if user_data.is_null() {
        return;
    }

    debug!("Dropping ExternalVerifier with address {:p}", user_data);

    // SAFETY: user_data is null-checked and was set to *mut ExternalVerifier in
    // ExternalVerifier::new().
    // This pointer value is ExternalVerifier::inner::user_data.
    // It's called from grpcio C++ implementation the following way:
    // ```
    //    if (external_verifier_->destruct != nullptr) {
    //      external_verifier_->destruct(external_verifier_->user_data);
    //    }
    //  ```
    unsafe {
        let _ = Pin::new_unchecked(Box::from_raw(user_data as *mut ExternalVerifier));
    }
}

/// Extracts request.target_name to an Option<String>
fn extract_target_name(
    request: &grpcio_sys::grpc_tls_custom_verification_check_request,
) -> anyhow::Result<Option<String>> {
    if request.target_name.is_null() {
        // It will be null when a server is verifying a client's certificate
        Ok(None)
    } else {
        // SAFETY: `target_name` is null-checked in the "if" arm and is a valid C string (see
        // SAFETY comments of `custom_verifier`.
        let c_str = unsafe { CStr::from_ptr(request.target_name) };
        Ok(Some(c_str.to_str().context("Failed to read request.target_name")?.to_owned()))
    }
}

/// Checks that the claimed VM name matches the expected one, if any.
fn check_claims_match_target(
    sd_cert_claims: SdCertificateClaims,
    target_name: Option<&str>,
) -> anyhow::Result<SdCertificateClaims> {
    match target_name {
        Some(target_name) => {
            let claimed_vm_name = sd_cert_claims.vm_name.to_string();
            if claimed_vm_name == target_name {
                Ok(sd_cert_claims)
            } else {
                Err(anyhow!("Target mismatch: claimed {claimed_vm_name}, expected {target_name}"))
            }
        }
        None => Ok(sd_cert_claims),
    }
}

/// This module defines the following structs: `ExternalVerifier`, `InternalVerifier`, `TlsOptions`.
///
/// `ExternalVerifier` represents a custom TLS verifier for gRPC. It wraps a `CredentialsChecker`
/// instance that is later opaquely provided to `custom_verify`.
///
/// `InternalVerifier` is an intermediate object to create `TlsOptions` with a custom verifier. It
/// refers to an `ExternalVerifier` instance without taking ownership.
///
/// `TlsOptions` is created from `InternalVerifier` taking its ownership.
///
/// Usage:
///
/// ```
/// let credentials_checker: CredentialsChecker = todo!();
/// let external_verifier = ExternalVerifier::new(credentials_checker);
/// let tls_options = TlsOptions::new(external_verifier);
/// ```

struct ExternalVerifier {
    credentials_checker: CredentialsChecker,
    inner: Mutex<Box<grpcio_sys::grpc_tls_certificate_verifier_external>>,
    _pin: PhantomPinned,
}

impl ExternalVerifier {
    pub fn new(credentials_checker: CredentialsChecker) -> Pin<Box<Self>> {
        let inner = Mutex::new(Box::new(grpcio_sys::grpc_tls_certificate_verifier_external {
            //user_data will be set below as a self pointer.
            user_data: std::ptr::null_mut(),
            verify: Some(custom_verifier),
            cancel: None,
            destruct: Some(destruct_external_verifier),
        }));

        let mut external_verifier =
            Box::pin(Self { credentials_checker, inner, _pin: PhantomPinned });

        // SAFETY: the raw address is used only to fill the user_data struct field
        let self_address: *mut c_void = unsafe {
            let mut_ref = external_verifier.as_mut().get_unchecked_mut();
            mut_ref as *mut ExternalVerifier as *mut c_void
        };

        // This will be the pointer passed to the `verify` and `destruct` C callbacks above.
        external_verifier.inner.lock().unwrap().user_data = self_address;

        debug!("Created ExternalVerifier with address {:p}", self_address);
        external_verifier
    }
}

// Wraps a raw pointer to `grpc_tls_certificate_verifier` object.
struct InternalVerifier {
    inner: NonNull<grpcio_sys::grpc_tls_certificate_verifier>,
}

impl InternalVerifier {
    fn new(external_verifier: Pin<Box<ExternalVerifier>>) -> anyhow::Result<Self> {
        // SAFETY: The passed external_verifier is guaranteed to be non-null by Box<>
        let Some(internal_verifier) = NonNull::new(unsafe {
            grpcio_sys::grpc_tls_certificate_verifier_external_create(
                external_verifier.inner.lock().unwrap().as_mut(),
            )
        }) else {
            bail!("Failed creating internal verifier.");
        };
        // Make Rust stop managing the memory of external_verifier.
        // SAFETY: We will take it back and drop it once the C++ code on the other side of the
        //         FFI calls our destruct_external_verifier() callback.
        let boxed_external_verifier: Box<ExternalVerifier> =
            unsafe { Pin::into_inner_unchecked(external_verifier) };
        let _raw_external_verifier = Box::into_raw(boxed_external_verifier);
        Ok(Self { inner: internal_verifier })
    }
}

impl Drop for InternalVerifier {
    fn drop(&mut self) {
        // SAFETY: `self.inner` was null-checked in `Self::new`.
        unsafe {
            grpcio_sys::grpc_tls_certificate_verifier_release(self.inner.as_ptr());
        }
    }
}

// Wraps a raw pointer to `grpc_tls_credentials_options` object.
struct TlsOptions {
    inner: NonNull<grpcio_sys::grpc_tls_credentials_options>,
}

impl TlsOptions {
    fn new(external_verifier: Pin<Box<ExternalVerifier>>) -> anyhow::Result<Self> {
        let Some(tls_options) =
            // SAFETY: There are no safety requirements set by `grpc_tls_credentials_options_create`
            // and the pointer returned by it is null-checked by `NonNull::new`.
            NonNull::new(unsafe { grpcio_sys::grpc_tls_credentials_options_create() })
        else {
            bail!("Failed creating TLS options");
        };
        // SAFETY: `tls_option` is null-checked by `NonNull::new`. `internal_verifier.inner` was
        // null-checked during its instantiation.
        unsafe {
            grpcio_sys::grpc_tls_credentials_options_set_certificate_verifier(
                tls_options.as_ptr(),
                InternalVerifier::new(external_verifier)?.inner.as_ptr(),
            );
        }
        Ok(Self { inner: tls_options })
    }

    // Takes the ownership of the `grpc_tls_credentials_options` object. The caller is responsible
    // for destroying it.
    fn into_inner(self) -> NonNull<grpcio_sys::grpc_tls_credentials_options> {
        ManuallyDrop::new(self).inner
    }

    fn set_certificate_provider(
        &mut self,
        certificate_provider: TlsCertificateProvider,
    ) -> anyhow::Result<()> {
        // SAFETY: Both pointers passed to `grpc_tls_credentials_options_set_certificate_provider`
        // are retrieved from `NonNull`. `tls_options` claims shared ownership of
        // `certificate_provider`, which releases (not destroy) its inner raw pointer.
        unsafe {
            grpcio_sys::grpc_tls_credentials_options_set_certificate_provider(
                self.inner.as_ptr(),
                certificate_provider.inner().as_ptr(),
            );
            grpcio_sys::grpc_tls_credentials_options_watch_identity_key_cert_pairs(
                self.inner.as_ptr(),
            );
        }
        Ok(())
    }

    fn set_client_cert_request_type(&mut self) {
        // SAFETY: `self.inner` is null-checked by `NonNull`, and is write-accessible and valid
        // during the execution of `grpc_tls_credentials_options_set_cert_request_type`,
        unsafe {
            grpcio_sys::grpc_tls_credentials_options_set_cert_request_type(
                self.inner.as_ptr(),
                grpcio_sys::grpc_ssl_client_certificate_request_type::GRPC_SSL_REQUEST_CLIENT_CERTIFICATE_BUT_DONT_VERIFY,
            );
        }
    }

    fn set_verify_server_certs(&mut self, verify_server_certs: bool) {
        // SAFETY:
        // - `self.inner` is retrieved from `NonNull`, guaranteeing it is non-null.
        // - `self.inner` was created by `grpc_tls_credentials_options_create` in `TlsOptions::new`,
        //   ensuring it is a valid pointer for the lifetime of `TlsOptions`.
        // - `set_verify_server_certs` takes `&mut self`, guaranteeing exclusive access and
        // preventing aliases for the duration of the call.
        unsafe {
            grpcio_sys::grpc_tls_credentials_options_set_verify_server_cert(
                self.inner.as_ptr(),
                verify_server_certs.into(),
            );
        }
    }
}

impl Drop for TlsOptions {
    fn drop(&mut self) {
        // SAFETY: `self.inner` was null-checked in `Self::new`.
        unsafe {
            grpcio_sys::grpc_tls_credentials_options_destroy(self.inner.as_ptr());
        }
    }
}

// RAII wrapper of a raw pointer to `grpc_tls_identity_pairs_create`.
struct TlsIdentityPairs {
    inner: NonNull<grpcio_sys::grpc_tls_identity_pairs>,
}

impl TlsIdentityPairs {
    fn new(private_key: &PKey<Private>, certificate: &X509) -> anyhow::Result<Self> {
        // Convert to the type that the C API consume.
        let private_key = CString::new(private_key.private_key_to_pem_pkcs8()?)?;
        let certificate = CString::new(certificate.to_pem()?)?;

        let Some(tls_identity_pairs) =
            // SAFETY: No unsafe requirements. The raw pointer returned by
            // `grpc_tls_identity_pairs_create` is null-checked by `NonNull`.
            NonNull::new(unsafe { grpcio_sys::grpc_tls_identity_pairs_create() })
        else {
            bail!("Failed creating TLS identity pair");
        };
        // SAFETY: `tls_identity_pairs` is created just above and null-checked by `NonNull`.
        // `private_key` and `cert_chain` validity are enforced by `CStr`.
        // `grpc_tls_identity_pairs_add_pair` copies the contents.
        unsafe {
            grpcio_sys::grpc_tls_identity_pairs_add_pair(
                tls_identity_pairs.as_ptr(),
                private_key.as_ptr(),
                certificate.as_ptr(),
            );
        }
        Ok(Self { inner: tls_identity_pairs })
    }

    // Takes the ownership of the `grpc_tls_identity_pairs`  object. The caller is responsible for
    // destroying it.
    fn into_inner(self) -> NonNull<grpcio_sys::grpc_tls_identity_pairs> {
        ManuallyDrop::new(self).inner
    }
}

impl Drop for TlsIdentityPairs {
    fn drop(&mut self) {
        // SAFETY: `self.inner` was null-checked in `Self::new`.
        unsafe {
            grpcio_sys::grpc_tls_identity_pairs_destroy(self.inner.as_ptr());
        }
    }
}

// RAII wrapper of a raw pointer to `grpc_tls_certificate_provider`.
struct TlsCertificateProvider {
    inner: NonNull<grpcio_sys::grpc_tls_certificate_provider>,
}

impl TlsCertificateProvider {
    fn new(tls_identity_pairs: TlsIdentityPairs) -> anyhow::Result<Self> {
        // SAFETY: `tls_identity_pairs.into_inner()` passes the ownership of the
        // `grpc_tls-identity_pairs` object to the function. The `grpc_tls_certificate_provider`
        // pointer returned is null-checked by NonNull.
        let Some(tls_certificate_provider) = NonNull::new(unsafe {
            grpcio_sys::grpc_tls_certificate_provider_static_data_create(
                std::ptr::null_mut(),
                tls_identity_pairs.into_inner().as_ptr(),
            )
        }) else {
            bail!("Failed creating TLS certificate provider.");
        };

        Ok(Self { inner: tls_certificate_provider })
    }

    // Returns pointer to the inner `grpc_tls_certificate_provider` object without taking ownership
    // of it.
    fn inner(&self) -> NonNull<grpcio_sys::grpc_tls_certificate_provider> {
        self.inner
    }
}

impl Drop for TlsCertificateProvider {
    fn drop(&mut self) {
        // SAFETY: `self.inner` was null-checked in `Self::new`.
        unsafe {
            grpcio_sys::grpc_tls_certificate_provider_release(self.inner.as_ptr());
        }
    }
}

/// `TransparentChannelCredentials` replicates the layout of `grpcio::ChannelCredentials` to enable
/// creating an instance of the latter from a raw `grpcio_sys::grpc_channel_credentials` pointer.
///
/// WARNING: These structs are `repr(Rust)` which gives almost no layout guarantees. The `transmute`
/// workaround relies on the fact that these structs consist of a single raw pointer field, and the
/// `test_channel_credentials_layouts` test.
struct TransparentChannelCredentials {
    _creds: *mut grpcio_sys::grpc_channel_credentials,
}

/// Creates `grpcio::ChannelCredentials` instance with a custom verifier based on
/// `credentials_checker`.
pub(crate) fn make_channel_credentials(
    credentials_checker: CredentialsChecker,
    certificate: &X509,
    pkey: &PKey<Private>,
) -> anyhow::Result<ChannelCredentials> {
    let certificate_provider =
        TlsCertificateProvider::new(TlsIdentityPairs::new(pkey, certificate)?)?;
    let external_verifier = ExternalVerifier::new(credentials_checker);
    let mut tls_options = TlsOptions::new(external_verifier)?;

    // Skip GRPC's server certificate verification, as we are using a custom verifier.
    tls_options.set_verify_server_certs(false);

    tls_options.set_certificate_provider(certificate_provider)?;

    // SAFETY: `TlsOptions::into_inner` returns `NonNull` wrapped pointer.
    let creds =
        unsafe { grpcio_sys::grpc_tls_credentials_create(tls_options.into_inner().as_ptr()) };
    if creds.is_null() {
        bail!("`grpc_tls_credentials_create` returned `null`.");
    }

    // SAFETY: At the time of writing `TransparentChannelCredentials` has the exact same layout as
    // `grpcio::ChannelCredentials`. The latter provides `as_mut_ptr()` function that returns the
    // raw pointer, but not the opposite.
    //
    // TODO(b/394576500): Remove `std::mem::transmute` and `TransparentChannelCredentials` once
    // it's possible to create a `ChannelCredentials` object from raw pointer.
    let channel_credentials = unsafe {
        std::mem::transmute::<TransparentChannelCredentials, ChannelCredentials>(
            TransparentChannelCredentials { _creds: creds },
        )
    };
    Ok(channel_credentials)
}

/// `TransparentServerCredentials` replicates the layout of `grpcio::ServerCredentials` to enable
/// creating an instance of the latter from a raw `grpcio_sys::grpc_server_credentials` pointer.
///
/// WARNING: These structs are `repr(Rust)` which gives almost no layout guarantees. The `transmute`
/// workaround partially relies on the `test_server_credentials_layouts` test.
/// `grpcio::ServerCredentials` consist of two fields, a raw `grpcio_sys::grpc_server_credentials`
/// pointer and a `Option<Box<Box<dyn crate::ServerCredentialsFetcher + Send + Sync>>>` field
/// behind a config.
struct TransparentServerCredentials {
    _creds: *mut grpcio_sys::grpc_server_credentials,
    _fetcher: Option<Box<()>>,
}

/// Creates `grpcio::ServerCredentials` instance with a custom verifier based on
/// `credentials_checker`.
pub(crate) fn make_server_credentials(
    credentials_checker: CredentialsChecker,
    certificate: &X509,
    pkey: &PKey<Private>,
) -> anyhow::Result<ServerCredentials> {
    let mut tls_options = TlsOptions::new(ExternalVerifier::new(credentials_checker))?;

    let certificate_provider =
        TlsCertificateProvider::new(TlsIdentityPairs::new(pkey, certificate)?)?;
    tls_options.set_certificate_provider(certificate_provider)?;
    tls_options.set_client_cert_request_type();

    // SAFETY: `tls_options` returns `NonNull` wrapped pointer.
    let creds = unsafe {
        grpcio_sys::grpc_tls_server_credentials_create(tls_options.into_inner().as_ptr())
    };
    if creds.is_null() {
        bail!("`grpc_tls_server_credentials_create` returned `null`.");
    }

    // SAFETY: At the time of writing `TransparentServerCredentials` has the exact same layout as
    // `grpcio::ServerCredentials`. The latter provides `as_mut_ptr()` function that returns the
    // raw pointer, but not the opposite.
    //
    // TODO(b/394576500): Remove `std::mem::transmute` and `TransparentServerCredentials` once
    // it's possible to create a `ServerCredentials` object from raw pointer.
    let server_credentials = unsafe {
        std::mem::transmute::<TransparentServerCredentials, ServerCredentials>(
            TransparentServerCredentials { _creds: creds, _fetcher: None },
        )
    };
    Ok(server_credentials)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::credentials_checker::tests::create_credentials_checker;
    use crate::mesh::sd_certificate_claims;
    use coset::CoseSign1;
    use crypto_rpc::cert::{build_ec_private_key, public_from_private};
    use std::{alloc::Layout, ffi::CString};

    #[test]
    fn test_custom_verifier_with_nulls() {
        // SAFETY: Parameters are allowed to be null.
        let result = unsafe {
            custom_verifier(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                None,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };

        assert_eq!(result, 1);
    }

    #[test]
    fn test_custom_verifier_without_request() {
        let mut sync_status = grpc_status_code::GRPC_STATUS_UNAUTHENTICATED;
        // SAFETY: Parameters are allowed to be null.
        let result = unsafe {
            custom_verifier(
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                None,
                std::ptr::null_mut(),
                &mut sync_status,
                std::ptr::null_mut(),
            )
        };

        assert_eq!(result, 1);
        assert_eq!(sync_status, grpc_status_code::GRPC_STATUS_UNAUTHENTICATED);
    }

    #[test]
    fn custom_verifier_ok() -> anyhow::Result<()> {
        use crate::mesh::credentials_checker::tests::{
            create_custom_credentials_checker, create_vvmconfig, SdClaimsBuilder,
        };
        use crate::mesh::dice_chains::AuthenticatedCsr;
        use crypto_rpc::cert::build_ec_private_key;
        use sample_dice_handover::create_random_uds;

        let vvm_config = create_vvmconfig()?;
        let uds = create_random_uds();
        let auth_csr = AuthenticatedCsr::create()?;

        let sd_private_key: PKey<Private> = build_ec_private_key()?.try_into()?;
        let sd_claims = SdClaimsBuilder::new(&vvm_config.remote_cfgs[0].name, &uds, &auth_csr)
            .with_sd_private_key(&sd_private_key)
            .build()?;

        let sd_cert_pem = sd_claims.to_x509(&sd_private_key)?.to_pem()?;

        let mut credentials_checker =
            create_custom_credentials_checker(&uds, vvm_config, sd_claims.rkp_csr.as_slice())?;

        let target_name_str = sd_claims.vm_name.to_string(); // "foo"
        let c_target_name = CString::new(target_name_str.as_str())?;
        let c_peer_cert = CString::new(sd_cert_pem.as_slice())?;
        let c_common_name = CString::new("")?; // Not used for target matching

        let mut request = grpcio_sys::grpc_tls_custom_verification_check_request {
            peer_info: grpcio_sys::grpc_tls_custom_verification_check_request_peer_info {
                common_name: c_common_name.as_ptr(),
                san_names:
                    grpcio_sys::grpc_tls_custom_verification_check_request_peer_info_san_names {
                        dns_names: std::ptr::null_mut(),
                        dns_names_size: 0,
                        ip_names: std::ptr::null_mut(),
                        ip_names_size: 0,
                        email_names: std::ptr::null_mut(),
                        email_names_size: 0,
                        uri_names: std::ptr::null_mut(),
                        uri_names_size: 0,
                    },
                peer_cert: c_peer_cert.as_ptr(),
                peer_cert_full_chain: std::ptr::null_mut(),
                verified_root_cert_subject: std::ptr::null_mut(),
            },
            target_name: c_target_name.as_ptr(),
        };

        let mut sync_status = grpc_status_code::GRPC_STATUS_UNAUTHENTICATED;
        let mut sync_error_details: *mut c_char = std::ptr::null_mut();

        // SAFETY: All pointers are valid and correctly initialized for the duration of the call.
        let result = unsafe {
            custom_verifier(
                &mut credentials_checker as *mut CredentialsChecker as *mut c_void,
                &mut request,
                None,                 // callback is not used in synchronous mode
                std::ptr::null_mut(), // callback_arg is not used in synchronous mode
                &mut sync_status,
                &mut sync_error_details,
            )
        };

        if !sync_error_details.is_null() {
            // SAFETY: Since sync_error_details isn't null, it points to an object allocated by
            // custom_verifier via gpr_zalloc, which means it must be valid to free with gpr_free.
            unsafe {
                grpcio_sys::gpr_free(sync_error_details as *mut c_void);
            }
        }

        assert_eq!(result, 1, "Expected synchronous check (return 1)");
        assert_eq!(sync_status, grpc_status_code::GRPC_STATUS_OK);

        Ok(())
    }

    #[test]
    fn test_tls_options() -> anyhow::Result<()> {
        let credentials_checker = create_credentials_checker()?;
        let external_verifier = ExternalVerifier::new(credentials_checker);
        let tls_options = TlsOptions::new(external_verifier);
        assert!(tls_options.is_ok());
        Ok(())
    }

    #[test]
    fn test_tls_options_consumed() -> anyhow::Result<()> {
        let credentials_checker = create_credentials_checker()?;
        let external_verifier = ExternalVerifier::new(credentials_checker);
        let tls_options = TlsOptions::new(external_verifier)?;
        let _inner = tls_options.into_inner();
        Ok(())
    }

    #[test]
    fn test_make_channel_credentials() -> anyhow::Result<()> {
        let subject_private_key = build_ec_private_key()?.try_into()?;
        let sd_claims = SdCertificateClaims {
            android_sdv: vec![43u8; 20 * 1024],
            android_sdv_signer_certificate: CoseSign1::default(),
            rkp_csr: vec![42u8; 20 * 1024],
            secure_world_signer_certificate_der: vec![],
            subject_public_key: public_from_private(&subject_private_key)?,
            uds_certs_cbor: None,
            vm_name: "instance1".to_owned().try_into()?,
        };
        let sd_cert = sd_claims.to_x509(&subject_private_key)?;

        let mut channel_credentials = make_channel_credentials(
            create_credentials_checker()?,
            &sd_cert,
            &subject_private_key,
        )?;
        assert!(!channel_credentials.as_mut_ptr().is_null());

        Ok(())
    }

    #[test]
    fn test_channel_credentials_layouts() {
        assert_eq!(
            Layout::new::<ChannelCredentials>(),
            Layout::new::<TransparentChannelCredentials>()
        );
        assert_eq!(std::mem::size_of::<ChannelCredentials>(), 8);
        assert_eq!(std::mem::align_of::<ChannelCredentials>(), 8);
    }

    #[test]
    fn test_make_server_credentials() -> anyhow::Result<()> {
        let subject_private_key = build_ec_private_key()?.try_into()?;
        let sd_claims = SdCertificateClaims {
            android_sdv: vec![43u8; 20 * 1024],
            android_sdv_signer_certificate: CoseSign1::default(),
            rkp_csr: vec![42u8; 20 * 1024],
            secure_world_signer_certificate_der: vec![],
            subject_public_key: public_from_private(&subject_private_key)?,
            uds_certs_cbor: None,
            vm_name: "instance1".to_owned().try_into()?,
        };
        let sd_cert = sd_claims.to_x509(&subject_private_key)?;

        let mut server_credentials =
            make_server_credentials(create_credentials_checker()?, &sd_cert, &subject_private_key)?;
        assert!(!server_credentials.as_mut_ptr().is_null());

        Ok(())
    }

    #[test]
    fn test_server_credentials_layouts() {
        assert_eq!(
            Layout::new::<ServerCredentials>(),
            Layout::new::<TransparentServerCredentials>()
        );
        assert_eq!(std::mem::size_of::<ServerCredentials>(), 16);
        assert_eq!(std::mem::align_of::<ServerCredentials>(), 8);
    }

    #[test]
    fn extract_target_name_ok() -> anyhow::Result<()> {
        let c_target_name = CString::new("foobar")?;

        let request = grpcio_sys::grpc_tls_custom_verification_check_request {
            peer_info: grpcio_sys::grpc_tls_custom_verification_check_request_peer_info {
                common_name: std::ptr::null_mut(),
                san_names:
                    grpcio_sys::grpc_tls_custom_verification_check_request_peer_info_san_names {
                        dns_names: std::ptr::null_mut(),
                        dns_names_size: 0,
                        ip_names: std::ptr::null_mut(),
                        ip_names_size: 0,
                        email_names: std::ptr::null_mut(),
                        email_names_size: 0,
                        uri_names: std::ptr::null_mut(),
                        uri_names_size: 0,
                    },
                peer_cert: std::ptr::null_mut(),
                peer_cert_full_chain: std::ptr::null_mut(),
                verified_root_cert_subject: std::ptr::null_mut(),
            },
            target_name: c_target_name.as_ptr(),
        };

        let target_name = extract_target_name(&request)?;
        assert_eq!(target_name, Some("foobar".to_owned()));
        Ok(())
    }

    #[test]
    fn check_claims_match_target_matching_target_ok() -> anyhow::Result<()> {
        check_claims_match_target(
            SdCertificateClaims {
                vm_name: "foobar".to_owned().try_into()?,
                ..sd_certificate_claims::create_default()?
            },
            Some("foobar"),
        )?;
        Ok(())
    }

    #[test]
    fn check_claims_match_target_no_target_ok() -> anyhow::Result<()> {
        check_claims_match_target(sd_certificate_claims::create_default()?, None)?;
        Ok(())
    }

    #[test]
    fn check_claims_match_target_mismatch_err() -> anyhow::Result<()> {
        let err = check_claims_match_target(
            SdCertificateClaims {
                vm_name: "foobar".to_owned().try_into()?,
                ..sd_certificate_claims::create_default()?
            },
            Some("something_else"),
        )
        .expect_err("check_claims_match_target() should have failed");

        assert_eq!(err.to_string(), "Target mismatch: claimed foobar, expected something_else");

        Ok(())
    }
}
