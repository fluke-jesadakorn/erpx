import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {
  FILES_CU_EARLIEST_ORIGINAL,
  FILES_CU_RECONCILIATION,
  FILES_CU_STORED_ORIGINALS,
  FILES_CU_UNRECONCILED_ORIGINALS,
  readFilesCommercialUsage,
  type CommercialStorageRow,
  type FilesCommercialActor,
} from "./native-lane-21/client";

/**
 * Files-owned commercial usage facts. The SQL, projection and response shape
 * belong to the native Files lane-21 axum router; these functions are a thin
 * transport that keeps the original caller contract. The `db` argument is
 * retained for signature compatibility only and never issues a fallback query.
 *
 * `actor` carries the caller's WorkContext holding identity. It is appended
 * (optional) so existing call sites keep compiling; the native client selects
 * exactly one registered Holding from it and fails closed when it is absent.
 */

export async function commercialStoredOriginals(
  _db: SqlDatabase,
  start: number,
  end: number,
  branchId: string | null,
  actor?: FilesCommercialActor,
): Promise<{ results: CommercialStorageRow[] }> {
  void _db;
  const result = await readFilesCommercialUsage(
    { kind: FILES_CU_STORED_ORIGINALS, start, end, branchId },
    actor,
  );
  return { results: result.rows };
}

export async function commercialUnreconciledOriginals(
  _db: SqlDatabase,
  end: number,
  branchId: string | null,
  actor?: FilesCommercialActor,
): Promise<{ count: number }> {
  void _db;
  const result = await readFilesCommercialUsage(
    { kind: FILES_CU_UNRECONCILED_ORIGINALS, end, branchId },
    actor,
  );
  return { count: result.count };
}

export async function commercialFileReconciliation(
  _db: SqlDatabase,
  actor?: FilesCommercialActor,
): Promise<{ ready: boolean }> {
  void _db;
  const result = await readFilesCommercialUsage({ kind: FILES_CU_RECONCILIATION }, actor);
  return { ready: result.ready };
}

export async function commercialEarliestOriginal(
  _db: SqlDatabase,
  actor?: FilesCommercialActor,
): Promise<{ earliest: number | null }> {
  void _db;
  const result = await readFilesCommercialUsage({ kind: FILES_CU_EARLIEST_ORIGINAL }, actor);
  return { earliest: result.earliest };
}
