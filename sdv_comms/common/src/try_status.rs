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

/// Macro to make FFI function blocks look more like idiomatic Rust code with ? operator based error-handling.
///
/// #### Before:
/// ```
/// pub unsafe extern "C" fn ASDV_something() -> sdv_status_t {
///    let a = {
///       let res: SdvResult<A> = do_one(x);
///       match res {
///           Result::Ok(a) => a,
///           Result::Err(status) => return status.into_raw(),
///       }
///    };
///    let b = {
///       let res: SdvResult<B> = do_two(a);
///       match res {
///           Result::Ok(b) => b,
///           Result::Err(status) => return status.into_raw(),
///       }
///    };

///    SdvStatus::Ok.into_raw()
/// }
/// ```
///
/// #### After:
/// ```
/// pub unsafe extern "C" fn ASDV_something() -> sdv_status_t {
///     try_status! {
///         let a = do_one(x)?;
///         let b = do_two(a)?;
///         Ok(b)
///     }
/// }
/// ```
#[macro_export]
macro_rules! try_status {
    { $($token:tt)* } => {{
        use $crate::try_status::__macro_imports as imported;

        // TODO: Replace this lambda with a Rust try_block, once the language feature is stabilised
        // Ref: https://doc.rust-lang.org/nightly/unstable-book/language-features/try-blocks.html
        #[allow(unused_mut)] // allows borrowing any captured variables mutably
        let mut try_status_lambda = || -> imported::SdvResult<()> {
            $($token)*
        };
        match try_status_lambda() {
            imported::SdvResult::Ok(_) => imported::SdvStatusCode::Ok.into_raw(),
            imported::SdvResult::Err(status) => {
                fn get_enclosing_fn_name() -> &'static str {
                    let name = ::core::any::type_name_of_val(&get_enclosing_fn_name);
                    &name[..name.len() - "::get_enclosing_fn_name".len()]
                }
                imported::error!("[Comms Stack error] {} @ {}", status, get_enclosing_fn_name());
                status.status_code().into_raw()
            }
        }
    }};
}

/// Private Module to expose imports for try_status macro.
/// This permits users of the macro to not depend on the macro's dependencies directly if needed.
pub mod __macro_imports {
    pub use log::error;
    pub use sdv_status_rs::{SdvResult, SdvStatusCode};
}

#[cfg(test)]
mod tests {
    use log::Level;
    use sdv_status_rs::{sdv_status_t, SdvStatus, SdvStatusCode};
    use std::sync::Mutex;

    struct TestLogger {
        logs: Mutex<Vec<(Level, String)>>,
    }

    impl log::Log for TestLogger {
        fn enabled(&self, _metadata: &log::Metadata) -> bool {
            true
        }

        fn log(&self, record: &log::Record) {
            let mut logs = self.logs.lock().unwrap();
            logs.push((record.level(), record.args().to_string()));
        }

        fn flush(&self) {}
    }

    static LOGGER: TestLogger = TestLogger { logs: Mutex::new(Vec::new()) };

    fn init_logger() {
        log::set_logger(&LOGGER).expect("Logger init should not fail.");
        log::set_max_level(log::LevelFilter::Error);
    }

    fn fetch_logs() -> Vec<(Level, String)> {
        let logs = LOGGER.logs.lock().unwrap();
        logs.clone()
    }

    #[test]
    fn test_try_status_returns_ok() {
        let val: sdv_status_t = try_status! {
            Ok(())
        };

        assert_eq!(val, SdvStatusCode::Ok.into_raw());
    }

    #[test]
    fn test_try_status_allows_mut_borrows() {
        let mut vec = Vec::new();
        let val: sdv_status_t = try_status! {
            vec.push(42);
            Ok(())
        };

        assert_eq!(val, SdvStatusCode::Ok.into_raw());
        assert_eq!(vec, vec![42]);
    }

    #[test]
    fn test_try_status_returns_err() {
        let val: sdv_status_t = try_status! {
            Err(SdvStatus::new(SdvStatusCode::Internal))
        };

        assert_eq!(val, SdvStatusCode::Internal.into_raw());
    }

    #[test]
    fn test_try_status_logs_err() {
        init_logger();

        let _: sdv_status_t = try_status! {
            Err(SdvStatus::new(SdvStatusCode::Internal))
        };

        let logs = fetch_logs();
        let expected_msg = "[Comms Stack error] SdvStatusCode::Internal(13) @ sdv_comms_common::try_status::tests::test_try_status_logs_err".to_string();
        assert_eq!(logs, vec![(Level::Error, expected_msg)]);
    }

    #[test]
    fn test_try_status_logs_err_with_message() {
        init_logger();

        let _: sdv_status_t = try_status! {
            Err(SdvStatus::with_message(SdvStatusCode::Internal, "Test Message"))
        };

        let logs = fetch_logs();
        let expected_msg = "[Comms Stack error] SdvStatusCode::Internal(13): Test Message @ sdv_comms_common::try_status::tests::test_try_status_logs_err_with_message".to_string();
        assert_eq!(logs, vec![(Level::Error, expected_msg)]);
    }
}
