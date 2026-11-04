import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireManageErpxContext} from "@erpx/server-identity_access/public/authorization";
import {errorResponse,ApiError} from "@erpx/server-runtime/public/http";
import {nativePlatformAdminFailure,nativePlatformMigrate} from "@erpx/feature-platform_administration/lib/native-platform-administration";
import {headers} from "next/headers";

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
    const data = await nativePlatformMigrate(work, platform, await headers(), body, key).catch((error) => {
      const failure = nativePlatformAdminFailure(error);
      if (failure) throw new ApiError(failure.status, failure.code, failure.message);
      throw error;
    });
    if (!data)
      throw new ApiError(
        503,
        "NATIVE_PLATFORM_ADMIN_UNAVAILABLE",
        "Platform Administration native access is not configured.",
      );
    return Response.json(canonicalDestinationLinks({ data }), {
      headers: { "Cache-Control": "private, no-store" },
    });
  } catch (error) {
    return errorResponse(error);
  }
}
