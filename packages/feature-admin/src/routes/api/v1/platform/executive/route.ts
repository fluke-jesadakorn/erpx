import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireManageErpxContext } from "@erpx/server-identity_access/public/authorization";
import {
  nativePlatformAdminFailure,
  nativePlatformExecutiveForPresentation,
  nativePlatformExecutiveSourcesForPresentation,
} from "@erpx/feature-platform_administration/lib/native-platform-administration";
import { headers as nextHeaders } from "next/headers";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";

export const dynamic = "force-dynamic";
const headers = { "Cache-Control": "private, no-store" };

function failure(error: unknown): never {
  const mapped = nativePlatformAdminFailure(error);
  if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
  throw error;
}

type ExecutiveSourceQuery = {
  metric: "failed-provisioning" | "abnormal-platform-services";
  group: string;
  from: string;
  to: string;
  page?: number;
};

/** Source reads require a known metric, an explicit group and UTC range. */
function sourceQuery(query: URLSearchParams): ExecutiveSourceQuery {
  const raw = query.get("metric");
  const metric = raw === "failed-provisioning" || raw === "abnormal-platform-services" ? raw : null;
  if (!metric)
    throw new ApiError(422, "PLATFORM_EXECUTIVE_SOURCE_INVALID", "Choose a supported Executive metric.");
  const page = query.get("page");
  return {
    metric,
    group: query.get("group") ?? "",
    from: query.get("from") ?? "",
    to: query.get("to") ?? "",
    page: page === null ? undefined : Number(page),
  };
}

export async function GET(request: Request) {
  try {
    const { work, platform } = await requireManageErpxContext();
    const query = new URL(request.url).searchParams;
    const incoming = await nextHeaders();
    const data = query.has("metric")
      ? await nativePlatformExecutiveSourcesForPresentation(work, platform, incoming, sourceQuery(query)).catch(failure)
      : await nativePlatformExecutiveForPresentation(work, platform, incoming, {
        from: query.get("from") ?? undefined,
        to: query.get("to") ?? undefined,
      }).catch(failure);
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (error) {
    return errorResponse(error);
  }
}
