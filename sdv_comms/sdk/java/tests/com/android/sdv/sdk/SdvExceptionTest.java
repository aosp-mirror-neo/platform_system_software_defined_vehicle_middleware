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
public final class SdvExceptionTest {

    @Test
    public void testFromStatusCode() {
        SdvException ex = new SdvException(SdvStatusCode.ASDV_STATUS_INVALID_ARGUMENT);

        assertEquals(SdvStatusCode.ASDV_STATUS_INVALID_ARGUMENT, ex.getStatusCode());
        assertEquals("ASDV_STATUS_INVALID_ARGUMENT", ex.getMessage());
    }

    @Test
    public void testFromStatusCodeAndMessage() {
        SdvException ex = new SdvException(SdvStatusCode.ASDV_STATUS_INTERNAL, "Something broke!");

        assertEquals(SdvStatusCode.ASDV_STATUS_INTERNAL, ex.getStatusCode());
        assertEquals("ASDV_STATUS_INTERNAL: Something broke!", ex.getMessage());
    }

    @Test
    public void testFromIntegerCode() {
        SdvException ex = new SdvException(/* ASDV_STATUS_DEADLINE_EXCEEDED*/ 4);

        assertEquals(SdvStatusCode.ASDV_STATUS_DEADLINE_EXCEEDED, ex.getStatusCode() );
        assertEquals("ASDV_STATUS_DEADLINE_EXCEEDED", ex.getMessage());
    }

    @Test
    public void testFromIntegerCodeAndMessage() {
        SdvException ex = new SdvException(/* ASDV_STATUS_NOT_FOUND */ 5, "Something failed!");

        assertEquals(SdvStatusCode.ASDV_STATUS_NOT_FOUND, ex.getStatusCode());
        assertEquals("ASDV_STATUS_NOT_FOUND: Something failed!", ex.getMessage());
    }
}
