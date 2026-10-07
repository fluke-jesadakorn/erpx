import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase, SqlStatement } from "@/server/modules/runtime/public/runtime/types";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { ApiError } from "@/server/modules/runtime/public/http";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { executeCommand, replayCommand } from "@/server/modules/workflow/public/commands";
import type { SupportCommandPorts } from "../application/command";
export function nativeSupportRuntime(actor: WorkContext, supplied?: SqlDatabase) { let db = supplied; const database = async () => db ??= (await holdingResources(actor.holdingId)).write; const ports: SupportCommandPorts = { authorize: requirePermission, error: (status, code, message) => new ApiError(status, code, message), now: Date.now, replay: async (a, key, operation, id, input) => replayCommand(await database(), a, key, operation, id, input), commit: async (c, mutation) => executeCommand(await database(), c.actor, c.key, c.operation, c.input, c.id, c.version, c.result, mutation.native as SqlStatement[], c.scope) }; return { database, ...ports }; }
