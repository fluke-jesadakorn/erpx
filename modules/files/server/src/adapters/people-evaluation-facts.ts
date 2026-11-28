import "server-only";
import type { SqlDatabase, SqlResult } from "@erpx/platform-runtime-server/public/runtime/types";
import {
  readNativePersonnelEvidence,
  type WorkEvidenceActor,
} from "./native-lane-22/personnel-client";

/**
 * Files-owned evaluation evidence facts. The two SELECTs formerly rendered
 * here are owned by the native Files axum router (lane 22, personnel family);
 * this adapter is a strict transport with no SQL and no fallback. `database`
 * is retained for call-site compatibility only.
 *
 * `actor` is appended (optional) so existing call sites keep compiling; the
 * native client selects exactly one registered Holding from it and fails
 * closed when it is absent.
 */
type EvaluationFileRow = { id: string; filename: string };

export function peopleEvaluationFilesFacts(database: SqlDatabase) {
  return {
    readEvaluationDetailFiles(
      companyId: string,
      assessmentId: string,
      actor?: WorkEvidenceActor,
    ): Promise<SqlResult<EvaluationFileRow>> {
      return readNativePersonnelEvidence(
        { ...(actor ?? {}), companyId },
        database,
        "evaluation-files",
        { assessmentId },
      ) as Promise<SqlResult<EvaluationFileRow>>;
    },
    readAddEvaluationEvidenceLink(
      sourceId: string | null,
      assessmentId: string,
      companyId: string,
      actor?: WorkEvidenceActor,
    ): Promise<Record<string, unknown> | null> {
      // A missing source never matched the retired statement either; resolve
      // it without a network call and without issuing any SQL.
      if (!sourceId) return Promise.resolve(null);
      return readNativePersonnelEvidence(
        { ...(actor ?? {}), companyId },
        database,
        "evaluation-link",
        { assessmentId, sourceId },
      ).then(result => result.results[0] ?? null);
    },
  };
}
