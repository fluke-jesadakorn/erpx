import type { SqlDatabase, SqlResult } from "@erpx/platform-runtime-server/public/runtime/types";
import {
  readNativePersonnelEvidence,
  type WorkEvidenceActor,
} from "./native-lane-22/personnel-client";

/**
 * Files-owned employee-request attachment facts. The single query formerly
 * rendered here is owned by the native Files axum router (lane 22, personnel
 * family); this adapter is a strict transport with no SQL and no fallback.
 * `database` is retained for call-site compatibility only.
 *
 * `actor` is appended (optional) so existing call sites keep compiling; it
 * supplies the caller's Company and registered Holding and fails closed when
 * absent.
 */
type EmployeeRequestAttachmentRow = {
  id: string;
  original_name: string;
  media_type: string;
  bytes: number;
  checksum_sha256: string;
  purpose: string;
  revision: number;
};

export function EmployeeRequestsFilesOwnedFacts(database: SqlDatabase) {
  return {
    readRequestAttachmentsEffect(
      id: string,
      actor?: WorkEvidenceActor,
    ): Promise<SqlResult<EmployeeRequestAttachmentRow>> {
      return readNativePersonnelEvidence(
        actor ?? { companyId: "" },
        database,
        "employee-request-attachments",
        { recordId: id },
      ) as Promise<SqlResult<EmployeeRequestAttachmentRow>>;
    },
  };
}
