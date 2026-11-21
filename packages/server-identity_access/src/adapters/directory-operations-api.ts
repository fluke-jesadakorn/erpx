import { z } from "zod";
import { directoryChangeInput } from "@erpx/feature-identity_access/lib/directory-operations";
import type { PostgresDatabase } from "@erpx/server-runtime/public/runtime/postgres";
import { iamActorFromWork } from "@erpx/server-identity_access/public/iam-context";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError } from "@erpx/server-runtime/public/http";
import { mappedFields, fieldSyncInput } from "@erpx/server-identity_access/public/directory-field-mapping";
import { connectorMappingInput } from "@erpx/server-identity_access/domain/directory-operations";
import {
  nativeDirectoryOperationsAvailable,
  nativeDirectoryOperationsCancel,
  nativeDirectoryOperationsConfigure,
  nativeDirectoryOperationsPreviewForHolding,
  nativeDirectoryOperationsReadForHolding,
  nativeDirectoryOperationsRefresh,
  nativeDirectoryOperationsRetry,
  nativeDirectoryOperationsSubmitForHolding,
} from "@erpx/server-identity_access/adapters/native-directory-operations";

function required(actor: { holdingId?: string | null }): string {
  if (!nativeDirectoryOperationsAvailable() || !actor.holdingId)
    throw new ApiError(
      503,
      "NATIVE_DIRECTORY_OPERATION_REQUIRED",
      "Directory operations require the native command service and a Holding context.",
    );
  return actor.holdingId;
}

export async function directoryOperationsApi(
  db: PostgresDatabase,
  context: WorkContext,
  path: string[],
  raw?: unknown,
  key?: string,
  headers?: Headers,
) {
  const actor = iamActorFromWork(context);
  if (raw === undefined) {
    const holdingId = required(actor);
    if (!headers)
      throw new ApiError(503, "NATIVE_DIRECTORY_OPERATION_REQUIRED", "Directory operations require the current request headers.");
    return nativeDirectoryOperationsReadForHolding(headers, holdingId);
  }
  if (!headers || !key)
    throw new ApiError(
      503,
      "NATIVE_DIRECTORY_OPERATION_REQUIRED",
      "Directory operations require the current request headers and Idempotency-Key.",
    );
  const input = z.record(z.string(), z.unknown()).parse(raw);
  if (path[0] === "fields" && ["preview", "sync"].includes(path[1]))
    return mappedFields(
      db,
      actor,
      fieldSyncInput.parse(input),
      key,
      path[1] === "preview",
      headers,
    );
  const holdingId = required(actor);
  if (path[0] === "refresh") {
    const v = z
      .object({
        connectionId: z.string(),
        expectedVersion: z.number().int().positive(),
      })
      .parse(input);
    return nativeDirectoryOperationsRefresh(headers, holdingId, v, key);
  }
  if (path[0] === "configure")
    return nativeDirectoryOperationsConfigure(
      headers,
      holdingId,
      z
        .object({
          connectionId: z.string(),
          expectedVersion: z.number().int().positive(),
          writeEnabled: z.boolean(),
          mappings: z.array(connectorMappingInput).max(50).optional(),
        })
        .parse(input),
      key,
    );
  if (path[0] === "preview")
    return nativeDirectoryOperationsPreviewForHolding(
      headers,
      holdingId,
      directoryChangeInput.parse(input),
    );
  if (path[0] === "submit" || path[0] === "apply")
    return nativeDirectoryOperationsSubmitForHolding(
      headers,
      holdingId,
      { ...directoryChangeInput.parse(input), previewToken: z.string().parse(input.previewToken) },
      key,
      path[0] === "apply",
    );
  if (path[0] === "cancel" || path[0] === "retry") {
    const v = z
      .object({ id: z.string(), expectedVersion: z.number().int().positive() })
      .parse(input);
    return (path[0] === "retry" ? nativeDirectoryOperationsRetry : nativeDirectoryOperationsCancel)(
      headers,
      holdingId,
      v,
      key,
    );
  }
  throw new ApiError(404, "NOT_FOUND", "Directory operation not found.");
}
