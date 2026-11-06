import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {z} from "zod";
import {requireManageErpxContext} from "@erpx/server-identity_access/public/authorization";
import {errorResponse,ApiError} from "@erpx/server-runtime/public/http";
import {activeHoldingBindings,holdingMigrationDatabase,runtimeEnv} from "../../../../../../../../../packages/server-runtime/src/public/runtime";
import {runHoldingMigrations} from "@erpx/server-platform_administration/public/platform-operations";
import {nativePlatformAdminFailure,nativePlatformAdministrationOrRetained,nativePlatformMigrate} from "@erpx/feature-platform_administration/lib/native-platform-administration";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import {headers} from "next/headers";

type ManageErpxPlatform = Awaited<ReturnType<typeof requireManageErpxContext>>["platform"];

const migrationCommand = z.object({
  holdingId: z.string().regex(/^[a-z0-9][a-z0-9._:-]{1,180}$/),
  track: z.literal("holding-write"),
  reason: z.string().trim().min(3).max(1000),
});

function failure(error: unknown): never {
  const mapped = nativePlatformAdminFailure(error);
  if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
  throw error;
}

async function retainedMigration(actor: ManageErpxPlatform, body: unknown, key: string) {
  const parsed = migrationCommand.safeParse(body);
  if (!parsed.success)
    throw new ApiError(
      422,
      "VALIDATION_FAILED",
      parsed.error.issues[0]?.message ?? "Invalid migration request.",
    );
  const binding = (await activeHoldingBindings()).find(
    (item) => item.id === parsed.data.holdingId,
  );
  if (!binding) throw new ApiError(404, "NOT_FOUND", "Holding not found.");
  const env = await runtimeEnv();
  const migrationDb = await holdingMigrationDatabase(binding.id).catch(() => {
    throw new ApiError(
      503,
      "MIGRATION_RUNNER_UNAVAILABLE",
      "This deployment has no provisioned migration runner for the selected Holding.",
    );
  });
  return runHoldingMigrations(
    actor,
    env.CONTROL_DB as PostgresDatabase,
    migrationDb as PostgresDatabase,
    parsed.data,
    key,
  );
}

export async function POST(request: Request) {
  try {
    const { platform, work } = await requireManageErpxContext();
    const key = request.headers.get("Idempotency-Key") ?? "";
    let body: unknown;
    try {
      body = await request.json();
    } catch {
      throw new ApiError(422, "INVALID_JSON", "Provide a migration command.");
    }
    if (!body || typeof body !== "object" || Array.isArray(body))
      throw new ApiError(422, "COMMAND_REQUIRED", "Provide a migration command.");
    const data = await nativePlatformAdministrationOrRetained(
      async () => nativePlatformMigrate(work, platform, await headers(), body, key),
      () => retainedMigration(platform, body, key),
    ).catch(failure);
    return Response.json(canonicalDestinationLinks({ data }), {
      headers: { "Cache-Control": "private, no-store" },
    });
  } catch (error) {
    return errorResponse(error);
  }
}
