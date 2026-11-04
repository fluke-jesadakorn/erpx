import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireManageErpxContext} from "@erpx/server-identity_access/public/authorization";
import {errorResponse,ApiError} from "@erpx/server-runtime/public/http";
import {nativePlatformAdminFailure,nativePlatformOperationsForPresentation} from "@erpx/feature-platform_administration/lib/native-platform-administration";
import {headers} from "next/headers";

export const dynamic = "force-dynamic";

export async function GET() {
  try {
    const { platform, work } = await requireManageErpxContext();
    const data = await nativePlatformOperationsForPresentation(work, platform, await headers()).catch((error) => {
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
    return Response.json(canonicalDestinationLinks({ data: data.operations }), {
      headers: { "Cache-Control": "private, no-store" },
    });
  } catch (error) {
    return errorResponse(error);
  }
}
