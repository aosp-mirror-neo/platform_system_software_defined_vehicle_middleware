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

/** Enum representing ASDV_STATUS_* codes */
public enum SdvStatusCode {
  ASDV_STATUS_OK(0),
  ASDV_STATUS_CANCELLED(1),
  ASDV_STATUS_UNKNOWN(2),
  ASDV_STATUS_INVALID_ARGUMENT(3),
  ASDV_STATUS_DEADLINE_EXCEEDED(4),
  ASDV_STATUS_NOT_FOUND(5),
  ASDV_STATUS_ALREADY_EXISTS(6),
  ASDV_STATUS_PERMISSION_DENIED(7),
  ASDV_STATUS_RESOURCE_EXHAUSTED(8),
  ASDV_STATUS_FAILED_PRECONDITION(9),
  ASDV_STATUS_ABORTED(10),
  ASDV_STATUS_OUT_OF_RANGE(11),
  ASDV_STATUS_UNIMPLEMENTED(12),
  ASDV_STATUS_INTERNAL(13),
  ASDV_STATUS_UNAVAILABLE(14),
  ASDV_STATUS_DATA_LOSS(15),
  ASDV_STATUS_UNAUTHENTICATED(16);


  /** {@return the integer code for this SdvStatusCode} */
  public int getCode() {
    return code;
  }

  /**
   * {@return the SdvStatusCode for an integer code}
   *
   * Returns ASDV_STATUS_UNKNOWN if code is unrecognized.
   *
   * @param code An integer ASDV_STATUS* code
   */
  public static final SdvStatusCode fromInteger(int code) {
    for (SdvStatusCode sc : SdvStatusCode.values()) {
      if (code == sc.code) {
        return sc;
      }
    }
    return SdvStatusCode.ASDV_STATUS_UNKNOWN;
  }

  private SdvStatusCode(int code) {
    this.code = code;
  }

  private final int code;
}