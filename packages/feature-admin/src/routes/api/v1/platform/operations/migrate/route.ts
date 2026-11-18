import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {z} from "zod";
import {requireManageErpxContext} from "@erpx/server-identity_access/public/authorization";
import {errorResponse,ApiError} from "@erpx/server-runtime/public/http";
import {nativePlatformAdminFailure,nativePlatformMigrate} from "@erpx/feature-platform_administration/lib/native-platform-administration";
import {headers} from "next/headers";

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
    const parsed = migrationCommand.safeParse(body);
    if (!parsed.success)
      throw new ApiError(
        422,
        "VALIDATION_FAILED",
        parsed.error.issues[0]?.message ?? "Invalid migration request.",
      );
    const data = await nativePlatformMigrate(
      work,
      platform,
      await headers(),
      parsed.data,
      key,
    ).catch(failure);
    return Response.json(canonicalDestinationLinks({ data }), {
      headers: { "Cache-Control": "private, no-store" },
    });
  } catch (error) {
    return errorResponse(error);
  }
}
