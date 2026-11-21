import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireManageErpxContext } from "@erpx/server-identity_access/public/authorization";
import { runtimeEnv } from "../../../../../../../../packages/server-runtime/src/public/runtime";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { platformExecutiveSnapshot, platformExecutiveSources, requirePlatformExecutive } from "@erpx/server-platform_administration/public/platform-executive";
import { headers as nextHeaders } from "next/headers";
import {
  nativePlatformExecutiveAvailable,
  nativePlatformExecutiveFailure,
  nativePlatformExecutiveOrRetained,
  nativePlatformExecutiveSnapshot,
  nativePlatformExecutiveSources,
} from "../../../../../public/native-platform-executive";

export const dynamic = "force-dynamic";
const headers = { "Cache-Control": "private, no-store" };

type ManageErpxContext = Awaited<ReturnType<typeof requireManageErpxContext>>;
type PlatformActor = ReturnType<typeof requirePlatformExecutive>;

function failure(error: unknown): never {
  const mapped = nativePlatformExecutiveFailure(error);
  if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
  throw error;
}

/** Source reads require a known metric, an explicit group and UTC range. */
function sourceQuery(query: URLSearchParams) {
  const metric = query.get("metric");
  if (metric !== "failed-provisioning" && metric !== "abnormal-platform-services")
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

/** Retained TypeScript read, used only while the native selector is unset. */
async function retained(actor: PlatformActor, query: URLSearchParams) {
  const env = await runtimeEnv();
  if (query.has("metric"))
    return platformExecutiveSources(actor, env.CONTROL_DB, env.CONTROL_READ_DB, {
      metric: query.get("metric"),
      label: query.get("group"),
      from: query.get("from"),
      to: query.get("to"),
      page: query.get("page") ?? "1",
    });
  const filters = query.has("from") || query.has("to")
    ? { from: query.get("from"), to: query.get("to") }
    : undefined;
  return platformExecutiveSnapshot(actor, env.CONTROL_DB, env.CONTROL_READ_DB, filters);
}

function native(
  context: ManageErpxContext,
  query: URLSearchParams,
  incoming: Pick<Headers, "get">,
) {
  if (query.has("metric"))
    return nativePlatformExecutiveSources(context.work, context.platform, incoming, sourceQuery(query));
  return nativePlatformExecutiveSnapshot(context.work, context.platform, incoming, {
    from: query.get("from") ?? undefined,
    to: query.get("to") ?? undefined,
  });
}

export async function GET(request: Request) {
  try {
    const beforeContext = await requireManageErpxContext();
    const before = requirePlatformExecutive(beforeContext.platform);
    const query = new URL(request.url).searchParams;
    const incoming = await nextHeaders();
    const data = await nativePlatformExecutiveOrRetained(
      nativePlatformExecutiveAvailable()
        ? () => native(beforeContext, query, incoming)
        : async () => undefined,
      () => retained(before, query),
    ).catch(failure);
    const afterContext = await requireManageErpxContext();
    const after = requirePlatformExecutive(afterContext.platform);
    if (before.assignmentId !== after.assignmentId
      || before.iam?.roleRevision !== after.iam?.roleRevision
      || before.iam?.identityVersion !== after.iam?.identityVersion)
      throw new ApiError(403, "PLATFORM_AUTHORITY_CHANGED", "Your Platform access changed during this read. Refresh and try again.");
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (error) {
    return errorResponse(error);
  }
}
