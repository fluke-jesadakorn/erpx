import "server-only";
import type { SqlDatabase, SqlResult } from "@erpx/platform-runtime-server/public/runtime/types";
import { attachmentsV2ReadOnly } from "@erpx/files-server/adapters/attachments/feature";
import { readNativeWorkEvidence, type WorkEvidenceRow } from "./native-lane-22/client";

/**
 * Files work-evidence facts. The two SELECTs formerly rendered here are now
 * owned by the native Files axum router (lane 22); this adapter is a strict
 * transport that emits no SQL and fails closed when the native service is
 * unavailable. `db` is retained only for call-site compatibility.
 */
type Facts = {
  recordId: string;
  companyId: string;
  holdingId?: string;
  holdingSlug?: string;
  origin?: string;
};
type EvidenceSetsRow = {
  id: string;
  set_id: string;
  purpose_key: string;
  original_name: string;
  bytes: number | null;
  media_type: string | null;
  source_state: string | null;
  link_state: string;
  preview_available: boolean;
  created_at: number;
};
type LegacyEvidenceRow = {
  id: string;
  original_name: string;
  bytes: number;
  media_type: string;
  set_id: string | null;
  purpose_key: string;
  source_state: string;
  link_state: string;
  preview_available: boolean;
  created_at: number;
};

export function workReadFilesFacts(db: SqlDatabase) {
  const actor = (facts: Facts) => ({
    companyId: facts.companyId,
    holdingId: facts.holdingId,
    holdingSlug: facts.holdingSlug,
    origin: facts.origin,
  });
  return {
    evidenceSets: (facts: Facts): Promise<SqlResult<EvidenceSetsRow>> =>
      readNativeWorkEvidence(actor(facts), db, facts.recordId, "sets", attachmentsV2ReadOnly()) as Promise<SqlResult<EvidenceSetsRow>>,
    legacyEvidence: (facts: Facts): Promise<SqlResult<LegacyEvidenceRow>> =>
      readNativeWorkEvidence(actor(facts), db, facts.recordId, "legacy", attachmentsV2ReadOnly()) as Promise<SqlResult<LegacyEvidenceRow>>,
  };
}

// Re-export for the strict TypeScript contract test: the native row shape is
// the authority, not a local projection.
export type { WorkEvidenceRow };
