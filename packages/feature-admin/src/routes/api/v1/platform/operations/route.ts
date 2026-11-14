import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {requireManageErpxContext} from "@erpx/server-identity_access/public/authorization";
import {errorResponse,ApiError} from "@erpx/server-runtime/public/http";
import {activeHoldingBindings,holdingResources,runtimeEnv} from "../../../../../../../../packages/server-runtime/src/public/runtime";
import {summarizePlatformOperations} from "@erpx/server-platform_administration/public/platform-operations";
import {nativePlatformAdminFailure,nativePlatformAdministrationOrRetained,nativePlatformOperationsForPresentation} from "@erpx/feature-platform_administration/lib/native-platform-administration";
import {headers} from "next/headers";

export const dynamic = "force-dynamic";

type ManageErpxPlatform = Awaited<ReturnType<typeof requireManageErpxContext>>["platform"];

function failure(error: unknown): never {
  const mapped = nativePlatformAdminFailure(error);
  if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
  throw error;
}

async function retainedOperations(actor: ManageErpxPlatform) {
  const env = await runtimeEnv();
  const bindings = (await activeHoldingBindings()).filter((row) => row.id !== "h003");
  return summarizePlatformOperations(
    actor,
    env.CONTROL_DB,
    await Promise.all(
      bindings.map(async (row) => ({
        id: row.id,
        slug: row.slug,
        displayName: row.displayName,
        db: (await holdingResources(row.id)).write,
      })),
    ),
  );
}

export async function GET() {
  try {
    const { platform, work } = await requireManageErpxContext();
    const data = await nativePlatformAdministrationOrRetained(
      async () => (await nativePlatformOperationsForPresentation(work, platform, await headers()))?.operations,
      () => retainedOperations(platform),
    ).catch(failure);
    return Response.json(canonicalDestinationLinks({ data }), {
      headers: { "Cache-Control": "private, no-store" },
    });
  } catch (error) {
    return errorResponse(error);
  }
}
