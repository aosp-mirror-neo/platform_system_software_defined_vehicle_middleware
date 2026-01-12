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

use sdv_comms_bindgen::{ASDV_PublisherUnitMetadata, ASDV_ServerUnitMetadata};
use sdv_status_rs::SdvStatusCode;

/// Defines the metadata used for Service Unit registration.
#[derive(Debug, PartialEq)]
pub enum UnitMetadata {
    /// Standardised metadata associated with a Server.
    Server(ServerUnitMetadata),
    /// Standardised metadata associated with a Publisher.
    Publisher(PublisherUnitMetadata),
}

/// Standardised metadata associated with a Server.
/// Owns a pointer of type sdv_comms_bindgen::ASDV_ServerUnitMetadata.
#[derive(Debug)]
pub struct ServerUnitMetadata(*mut ASDV_ServerUnitMetadata);

static_assertions::assert_impl_all!(ServerUnitMetadata: Send, Sync);

/// Safety: A `ServerUnitMetadata` is a wrapper around `ASDV_ServerUnitMetadata` which should
/// be threadsafe.
unsafe impl Send for ServerUnitMetadata {}

/// Safety: A `ServerUnitMetadata` is a wrapper around `ASDV_ServerUnitMetadata` which should
/// be threadsafe.
unsafe impl Sync for ServerUnitMetadata {}

impl Default for ServerUnitMetadata {
    fn default() -> Self {
        ServerUnitMetadata::new()
    }
}

impl ServerUnitMetadata {
    /// # Safety
    /// This function borrows a reference to the ServerUnitMetadata as the underlying pointer.
    /// The returned pointer MUST NOT be mutated by the caller, and MUST NOT be deleted by the caller.
    #[inline]
    pub unsafe fn as_raw(&self) -> *const ASDV_ServerUnitMetadata {
        self.0 as _
    }

    /// # Safety
    /// This function borrows a reference to the ServerUnitMetadata as the underlying mutable pointer.
    /// The returned pointer MUST NOT be deleted by the caller.
    #[inline]
    pub unsafe fn as_raw_mut(&mut self) -> *mut ASDV_ServerUnitMetadata {
        self.0
    }

    /// Create a new [ServerUnitMetadata].
    pub fn new() -> Self {
        // SAFETY: result of `ASDV_ServerUnitMetadata_create` is guaranteed to be a valid pointer.
        Self(unsafe { sdv_comms_bindgen::ASDV_ServerUnitMetadata_create() })
    }

    /// Sets the version field of this [ServerUnitMetadata].
    pub fn set_version(&mut self, version: u32) {
        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status =
            unsafe { sdv_comms_bindgen::ASDV_ServerUnitMetadata_setVersion(self.0, &version) };

        SdvStatusCode::from(status).expect("ServerUnitMetadata::set_version() failed");
    }

    /// Returns the version field of this [ServerUnitMetadata].
    pub fn get_version(&self) -> u32 {
        let mut version: u32 = 0;

        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status =
            unsafe { sdv_comms_bindgen::ASDV_ServerUnitMetadata_getVersion(self.0, &mut version) };

        SdvStatusCode::from(status).expect("ServerUnitMetadata::get_version() failed");

        version
    }
}

impl Drop for ServerUnitMetadata {
    fn drop(&mut self) {
        // SAFETY: `self.0` is guaranteed to be a valid pointer, before
        // `ASDV_ServerUnitMetadata_destroy` is called.
        unsafe { sdv_comms_bindgen::ASDV_ServerUnitMetadata_destroy(self.0) }
    }
}

impl PartialEq for ServerUnitMetadata {
    fn eq(&self, other: &ServerUnitMetadata) -> bool {
        let mut are_equal = false;

        // SAFETY: `self.0` and `other.0` are guaranteed to be valid pointers, before
        // `ASDV_ServerUnitMetadata_destroy` is called. `are_equal` points to a boolean.
        // All parameters outlive the function call.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_ServerUnitMetadata_equals(
                self.as_raw(),
                other.as_raw(),
                &mut are_equal,
            )
        };
        SdvStatusCode::from(status).expect("ASDV_ServerUnitMetadata_equals() failed");
        are_equal
    }
}

impl Clone for ServerUnitMetadata {
    fn clone(&self) -> ServerUnitMetadata {
        let mut clone = ServerUnitMetadata::new();
        // SAFETY: `self.0` is guaranteed to be a valid pointer, before
        // `ASDV_ServerUnitMetadata_destroy` is called.
        clone.0 = unsafe { sdv_comms_bindgen::ASDV_ServerUnitMetadata_clone(self.0) };
        clone
    }
}

/// Standardised metadata associated with a Publisher.
/// Owns a pointer of type sdv_comms_bindgen::ASDV_PublisherUnitMetadata.
#[derive(Debug)]
pub struct PublisherUnitMetadata(*mut ASDV_PublisherUnitMetadata);

/// Safety: A `PublisherUnitMetadata` is a wrapper around `ASDV_PublisherUnitMetadata` which should
/// be threadsafe.
unsafe impl Send for PublisherUnitMetadata {}

/// Safety: A `PublisherUnitMetadata` is a wrapper around `ASDV_PublisherUnitMetadata` which should
/// be threadsafe.
unsafe impl Sync for PublisherUnitMetadata {}

impl Default for PublisherUnitMetadata {
    fn default() -> Self {
        PublisherUnitMetadata::new()
    }
}

impl PublisherUnitMetadata {
    /// # Safety
    /// This function borrows a reference to the PublisherUnitMetadata as the underlying pointer.
    /// The returned pointer MUST NOT be mutated by the caller, and MUST NOT be deleted by the caller.
    #[inline]
    pub unsafe fn as_raw(&self) -> *const ASDV_PublisherUnitMetadata {
        self.0 as _
    }

    /// # Safety
    /// This function borrows a reference to the PublisherUnitMetadata as the underlying mutable pointer.
    /// The returned pointer MUST NOT be deleted by the caller.
    #[inline]
    pub unsafe fn as_raw_mut(&mut self) -> *mut ASDV_PublisherUnitMetadata {
        self.0
    }

    /// Create a new [ServerUnitMetadata].
    pub fn new() -> PublisherUnitMetadata {
        // SAFETY: result of `ASDV_PublisherUnitMetadata_create` is guaranteed to be a valid pointer.
        Self(unsafe { sdv_comms_bindgen::ASDV_PublisherUnitMetadata_create() })
    }

    /// Sets the version field of this [PublisherUnitMetadata].
    pub fn set_version(&mut self, version: u32) {
        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status =
            unsafe { sdv_comms_bindgen::ASDV_PublisherUnitMetadata_setVersion(self.0, &version) };

        SdvStatusCode::from(status).expect("ServerUnitMetadata::set_version() failed");
    }

    /// Sets the message_size field of this [PublisherUnitMetadata].
    pub fn set_message_size(&mut self, message_size: u32) {
        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_PublisherUnitMetadata_setMessageSize(self.0, &message_size)
        };

        SdvStatusCode::from(status).expect("ServerUnitMetadata::set_message_size() failed");
    }

    /// Sets the message_count field of this [PublisherUnitMetadata].
    pub fn set_message_count(&mut self, message_count: u32) {
        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_PublisherUnitMetadata_setMessageCount(self.0, &message_count)
        };

        SdvStatusCode::from(status).expect("ServerUnitMetadata::set_message_count() failed");
    }

    /// Returns the version field of this [PublisherUnitMetadata].
    pub fn get_version(&self) -> u32 {
        let mut version: u32 = 0;

        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_PublisherUnitMetadata_getVersion(self.0, &mut version)
        };

        SdvStatusCode::from(status).expect("ServerUnitMetadata::get_version() failed");

        version
    }

    /// Returns the message_size field of this [PublisherUnitMetadata].
    pub fn get_message_size(&self) -> u32 {
        let mut message_size: u32 = 0;

        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_PublisherUnitMetadata_getMessageSize(self.0, &mut message_size)
        };

        SdvStatusCode::from(status).expect("ServerUnitMetadata::get_message_size() failed");

        message_size
    }

    /// Returns the message_count field of this [PublisherUnitMetadata].
    pub fn get_message_count(&self) -> u32 {
        let mut message_count: u32 = 0;

        // SAFETY: `self.0` is guaranteed to be a valid pointer.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_PublisherUnitMetadata_getMessageCount(
                self.0,
                &mut message_count,
            )
        };

        SdvStatusCode::from(status).expect("ServerUnitMetadata::get_message_count() failed");

        message_count
    }
}

impl Drop for PublisherUnitMetadata {
    fn drop(&mut self) {
        // SAFETY: `self.0` is guaranteed to be a valid pointer, before
        // `ASDV_PublisherUnitMetadata_destroy` is called.
        unsafe { sdv_comms_bindgen::ASDV_PublisherUnitMetadata_destroy(self.0) }
    }
}

impl PartialEq for PublisherUnitMetadata {
    fn eq(&self, other: &PublisherUnitMetadata) -> bool {
        let mut are_equal = false;

        // SAFETY: `self.0` and `other.0` are guaranteed to be valid pointers, before
        // `ASDV_PublisherUnitMetadata_destroy` is called. `are_equal` points to a boolean.
        // All parameters outlive the function call.
        let status = unsafe {
            sdv_comms_bindgen::ASDV_PublisherUnitMetadata_equals(
                self.as_raw(),
                other.as_raw(),
                &mut are_equal,
            )
        };
        SdvStatusCode::from(status).expect("ASDV_PublisherUnitMetadata_equals() failed");
        are_equal
    }
}

impl Clone for PublisherUnitMetadata {
    fn clone(&self) -> PublisherUnitMetadata {
        let mut clone = PublisherUnitMetadata::new();
        // SAFETY: `self.0` is guaranteed to be a valid pointer, before
        // `ASDV_ServerUnitMetadata_destroy` is called.
        clone.0 = unsafe { sdv_comms_bindgen::ASDV_PublisherUnitMetadata_clone(self.0) };
        clone
    }
}

#[cfg(test)]
mod server_metadata_tests {
    use super::*;
    use googletest::{assert_that, prelude::eq};

    #[test]
    fn test_create_destroy() {
        let _args = ServerUnitMetadata::default();

        // Implicit drop is happening here:
        //
        // drop(args);
    }

    #[test]
    fn test_get_default_version() {
        let args = ServerUnitMetadata::default();

        assert_that!(args.get_version(), eq(0));
    }

    #[test]
    fn test_get_set_version() {
        let mut args = ServerUnitMetadata::default();

        args.set_version(42);

        assert_that!(args.get_version(), eq(42));
    }

    #[test]
    fn test_eq() {
        let mut args1 = ServerUnitMetadata::default();
        let mut args2 = ServerUnitMetadata::default();

        args1.set_version(40);
        args2.set_version(40);

        assert_that!(args1, eq(&args2));
    }
}

#[cfg(test)]
mod publisher_metadata_tests {
    use super::*;
    use googletest::{assert_that, prelude::eq};

    #[test]
    fn test_create_destroy() {
        let _args = PublisherUnitMetadata::default();

        // Implicit drop is happening here:
        //
        // drop(args);
    }

    #[test]
    fn test_get_default_version() {
        let args = PublisherUnitMetadata::default();

        assert_that!(args.get_version(), eq(0));
        assert_that!(args.get_message_count(), eq(0));
        assert_that!(args.get_message_size(), eq(0));
    }

    #[test]
    fn test_get_set_version() {
        let mut args = PublisherUnitMetadata::default();

        args.set_version(42);

        assert_that!(args.get_version(), eq(42));
    }

    #[test]
    fn test_get_default_message_size() {
        let args = PublisherUnitMetadata::default();

        assert_that!(args.get_message_size(), eq(0));
    }

    #[test]
    fn test_get_set_message_size() {
        let mut args = PublisherUnitMetadata::default();

        args.set_message_size(42);

        assert_that!(args.get_message_size(), eq(42));
    }

    #[test]
    fn test_get_default_message_count() {
        let args = PublisherUnitMetadata::default();

        assert_that!(args.get_message_count(), eq(0));
    }

    #[test]
    fn test_get_set_message_count() {
        let mut args = PublisherUnitMetadata::default();

        args.set_message_count(42);

        assert_that!(args.get_message_count(), eq(42));
    }

    #[test]
    fn test_eq() {
        let mut args1 = PublisherUnitMetadata::default();
        let mut args2 = PublisherUnitMetadata::default();

        args1.set_message_count(42);
        args2.set_message_count(42);

        args1.set_message_size(41);
        args2.set_message_size(41);

        args1.set_version(40);
        args2.set_version(40);

        assert_that!(args1, eq(&args2));
    }
}
