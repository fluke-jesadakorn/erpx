import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireManageErpxContext} from "@erpx/server-identity_access/public/authorization";
import {errorResponse,ApiError} from "@erpx/server-runtime/public/http";
import {nativePlatformAdminFailure,nativePlatformOperationsForPresentation} from "@erpx/feature-platform_administration/lib/native-platform-administration";
import {headers} from "next/headers";

export const dynamic = "force-dynamic";

function failure(error: unknown): never {
  const mapped = nativePlatformAdminFailure(error);
  if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
  throw error;
}

export async function GET() {
  try {
    const { platform, work } = await requireManageErpxContext();
    const { operations } = await nativePlatformOperationsForPresentation(
      work,
      platform,
      await headers(),
    ).catch(failure);
    return Response.json(canonicalDestinationLinks({ data: operations }), {
      headers: { "Cache-Control": "private, no-store" },
    });
  } catch (error) {
    return errorResponse(error);
  }
}
