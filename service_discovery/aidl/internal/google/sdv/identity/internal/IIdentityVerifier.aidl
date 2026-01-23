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

package google.sdv.identity.internal;

import google.sdv.identity.LocalServiceFqin;
import google.sdv.identity.ProcessIdentifier;

/** The interface for checking whether the given identity is available for the process. */
interface IIdentityVerifier {
    /**
     * Checks if the given FQIN can be used by the process.
     *
     * @param[in] fqin              the FQIN requested by the caller service bundle (service, app,
     *                              etc.).
     * @param[in] processIdentifier the process requesting the FQIN.
     * @return            a boolean indicating whether the FQIN is allowed to be used by the
     *                    process.
     */
    boolean verifyFqinForProcess(in LocalServiceFqin fqin, in ProcessIdentifier processIdentifier);
}
