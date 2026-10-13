import { z } from "zod";
import { directoryChangeInput } from "@/lib/identity_access/directory-operations";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";
import { iamActorFromWork } from "@/server/modules/identity_access/public/iam-context";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError } from "@/server/modules/runtime/public/http";
import { mappedFields, fieldSyncInput } from "@/server/modules/identity_access/public/directory-field-mapping";
import {
  directoryAutomationData,
  refreshDirectoryInventory,
  configureDirectoryAutomation,
  previewDirectoryChange,
  submitDirectoryChange,
  cancelDirectoryOperation,
  retryDirectoryOperation,
  connectorMappingInput,
} from "@/server/modules/identity_access/public/directory-operations";

export async function directoryOperationsApi(
  db: PostgresDatabase,
  context: WorkContext,
  path: string[],
  raw?: unknown,
  key?: string,
) {
  const actor = iamActorFromWork(context);
  if (raw === undefined) return directoryAutomationData(db, actor);
  const input = z.record(z.string(), z.unknown()).parse(raw);
  if (path[0] === "fields" && ["preview", "sync"].includes(path[1]))
    return mappedFields(
      db,
      actor,
      fieldSyncInput.parse(input),
      key!,
      path[1] === "preview",
    );
  if (path[0] === "refresh") {
    const v = z
      .object({
        connectionId: z.string(),
        expectedVersion: z.number().int().positive(),
      })
      .parse(input);
    return refreshDirectoryInventory(
      db,
      actor,
      v.connectionId,
      v.expectedVersion,
      key!,
    );
  }
  if (path[0] === "configure")
    return configureDirectoryAutomation(
      db,
      actor,
      z
        .object({
          connectionId: z.string(),
          expectedVersion: z.number().int().positive(),
          writeEnabled: z.boolean(),
          mappings: z.array(connectorMappingInput).max(50).optional(),
        })
        .parse(input),
      key!,
    );
  if (path[0] === "preview")
    return previewDirectoryChange(db, actor, directoryChangeInput.parse(input));
  if (path[0] === "submit" || path[0] === "apply")
    return submitDirectoryChange(
      db,
      actor,
      directoryChangeInput.parse(input),
      z.string().parse(input.previewToken),
      key!,
      path[0] === "apply",
    );
  if (path[0] === "cancel" || path[0] === "retry") {
    const v = z
      .object({ id: z.string(), expectedVersion: z.number().int().positive() })
      .parse(input);
    return (path[0] === "retry" ? retryDirectoryOperation : cancelDirectoryOperation)(db, actor, v.id, v.expectedVersion, key!);
  }
  throw new ApiError(404, "NOT_FOUND", "Directory operation not found.");
}
