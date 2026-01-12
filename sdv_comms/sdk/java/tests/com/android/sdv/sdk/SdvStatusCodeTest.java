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

import static org.junit.Assert.assertEquals;

import org.junit.Test;
import org.junit.runner.RunWith;
import org.junit.runners.JUnit4;

@RunWith(JUnit4.class)
public final class SdvStatusCodeTest {

    @Test
    public void testfromInteger() {
        assertEquals(SdvStatusCode.fromInteger(0), SdvStatusCode.ASDV_STATUS_OK);
        assertEquals(SdvStatusCode.fromInteger(1), SdvStatusCode.ASDV_STATUS_CANCELLED);
        assertEquals(SdvStatusCode.fromInteger(2), SdvStatusCode.ASDV_STATUS_UNKNOWN);
        assertEquals(SdvStatusCode.fromInteger(3), SdvStatusCode.ASDV_STATUS_INVALID_ARGUMENT);
        assertEquals(SdvStatusCode.fromInteger(4), SdvStatusCode.ASDV_STATUS_DEADLINE_EXCEEDED);
        assertEquals(SdvStatusCode.fromInteger(5), SdvStatusCode.ASDV_STATUS_NOT_FOUND);
        assertEquals(SdvStatusCode.fromInteger(6), SdvStatusCode.ASDV_STATUS_ALREADY_EXISTS);
        assertEquals(SdvStatusCode.fromInteger(7), SdvStatusCode.ASDV_STATUS_PERMISSION_DENIED);
        assertEquals(SdvStatusCode.fromInteger(8), SdvStatusCode.ASDV_STATUS_RESOURCE_EXHAUSTED);
        assertEquals(SdvStatusCode.fromInteger(9), SdvStatusCode.ASDV_STATUS_FAILED_PRECONDITION);
        assertEquals(SdvStatusCode.fromInteger(10), SdvStatusCode.ASDV_STATUS_ABORTED);
        assertEquals(SdvStatusCode.fromInteger(11), SdvStatusCode.ASDV_STATUS_OUT_OF_RANGE);
        assertEquals(SdvStatusCode.fromInteger(12), SdvStatusCode.ASDV_STATUS_UNIMPLEMENTED);
        assertEquals(SdvStatusCode.fromInteger(13), SdvStatusCode.ASDV_STATUS_INTERNAL);
        assertEquals(SdvStatusCode.fromInteger(14), SdvStatusCode.ASDV_STATUS_UNAVAILABLE);
        assertEquals(SdvStatusCode.fromInteger(15), SdvStatusCode.ASDV_STATUS_DATA_LOSS);
        assertEquals(SdvStatusCode.fromInteger(16), SdvStatusCode.ASDV_STATUS_UNAUTHENTICATED);

        assertEquals(SdvStatusCode.fromInteger(-1), SdvStatusCode.ASDV_STATUS_UNKNOWN);
        assertEquals(SdvStatusCode.fromInteger(17), SdvStatusCode.ASDV_STATUS_UNKNOWN);
    }
}
