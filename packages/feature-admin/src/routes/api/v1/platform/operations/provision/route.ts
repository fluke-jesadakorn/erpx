import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireManageErpxContext} from "@erpx/server-identity_access/public/authorization";
import {errorResponse,ApiError} from "@erpx/server-runtime/public/http";
import {nativePlatformAdminFailure,nativePlatformOperationsForPresentation,nativePlatformProvision} from "@erpx/feature-platform_administration/lib/native-platform-administration";
import {headers} from "next/headers";

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
      throw new ApiError(422, "INVALID_JSON", "Provide a provisioning command.");
    }
    if (!body || typeof body !== "object" || Array.isArray(body))
      throw new ApiError(422, "COMMAND_REQUIRED", "Provide a provisioning command.");
    const data = await nativePlatformProvision(work, platform, await headers(), body, key).catch(failure);
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

export async function GET() {
  try {
    const { platform, work } = await requireManageErpxContext();
    const snapshot = await nativePlatformOperationsForPresentation(work, platform, await headers()).catch(failure);
    if (!snapshot)
      throw new ApiError(
        503,
        "NATIVE_PLATFORM_ADMIN_UNAVAILABLE",
        "Platform Administration native access is not configured.",
      );
    return Response.json(canonicalDestinationLinks({ data: snapshot.provisioning }), {
      headers: { "Cache-Control": "private, no-store" },
    });
  } catch (error) {
    return errorResponse(error);
  }
}
