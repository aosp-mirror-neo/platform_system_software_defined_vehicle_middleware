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

package com.android.sdv.sdk;


/** Exception representing SDV-specific error conditions. */
public final class SdvException extends RuntimeException {

  /** {@return the SdvStatusCode for this exception} */
  public SdvStatusCode getStatusCode() {
    return statusCode;
  }

  /** Construct SdvException from SdvStatusCode only */
  SdvException(SdvStatusCode statusCode) {
    super(statusCode.toString());
    this.statusCode = statusCode;
  }

  /** Construct SdvException from SdvStatusCode with detailed message */
  SdvException(SdvStatusCode statusCode, String message) {
    super(statusCode.toString() + ": " + message);
    this.statusCode = statusCode;
  }

  /** Construct SdvException from integer status code only */
  SdvException(int code) {
    super(SdvStatusCode.fromInteger(code).toString());
    this.statusCode =  SdvStatusCode.fromInteger(code);
  }

  /** Construct SdvException from integer status code with detailed message */
  SdvException(int code, String message) {
    super(SdvStatusCode.fromInteger(code).toString() + ": " + message);
    this.statusCode = SdvStatusCode.fromInteger(code);
  }

  private final SdvStatusCode statusCode;
}
