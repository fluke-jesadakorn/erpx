import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { z } from "zod";
import { requireManageErpxContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse, ApiError } from "@/server/modules/runtime/public/http";
import { activeHoldingBindings, holdingMigrationDatabase, runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { runHoldingMigrations } from "@/server/modules/platform_administration/public/platform-operations";
import type { PostgresDatabase } from "@/server/modules/runtime/public/runtime/postgres";

const schema = z.object({
  holdingId: z.string().regex(/^[a-z0-9][a-z0-9._:-]{1,180}$/),
  track: z.literal("holding-write"),
  reason: z.string().trim().min(3).max(1000),
});

export async function POST(request: Request) {
  try {
    const { platform: actor } = await requireManageErpxContext();
    const key = request.headers.get("Idempotency-Key") ?? "";
    const parsed = schema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid migration request.");
    const binding = (await activeHoldingBindings()).find((item) => item.id === parsed.data.holdingId);
    if (!binding) throw new ApiError(404, "NOT_FOUND", "Holding not found.");
    const env = await runtimeEnv();
    const migrationDb = await holdingMigrationDatabase(binding.id).catch(() => {
      throw new ApiError(503, "MIGRATION_RUNNER_UNAVAILABLE", "This deployment has no provisioned migration runner for the selected Holding.");
    });
    const result = await runHoldingMigrations(
      actor,
      env.CONTROL_DB as PostgresDatabase,
      migrationDb as PostgresDatabase,
      parsed.data,
      key,
    );
    return Response.json(canonicalDestinationLinks({ data: result }), { headers: { "Cache-Control": "no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}
