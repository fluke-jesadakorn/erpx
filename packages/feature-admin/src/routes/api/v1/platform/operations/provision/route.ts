import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {z} from "zod";
import {requireManageErpxContext} from "@erpx/server-identity_access/public/authorization";
import {errorResponse,ApiError} from "@erpx/server-runtime/public/http";
import {resourceSlots} from "@erpx/feature-runtime/lib/holding-registry";
import {holdingResources,runtimeEnv} from "../../../../../../../../../packages/server-runtime/src/public/runtime";
import {platformProvisioningSnapshot,provisionHolding} from "@erpx/server-platform_administration/public/platform-operations";
import {nativePlatformAdminFailure,nativePlatformAdministrationOrRetained,nativePlatformOperationsForPresentation,nativePlatformProvision} from "@erpx/feature-platform_administration/lib/native-platform-administration";
import type {PostgresDatabase} from "@erpx/server-runtime/public/runtime/postgres";
import {headers} from "next/headers";

type ManageErpxPlatform = Awaited<ReturnType<typeof requireManageErpxContext>>["platform"];

const provisioningCommand = z.object({
  holdingId: z.string().regex(/^[a-z0-9]{3,24}$/),
  slug: z.string().regex(/^[a-z][a-z0-9-]{2,39}$/),
  displayName: z.string().trim().min(2).max(120),
  reason: z.string().trim().min(10).max(1000),
});

function failure(error: unknown): never {
  const mapped = nativePlatformAdminFailure(error);
  if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
  throw error;
}

async function retainedProvisioningSnapshot() {
  const env = await runtimeEnv();
  return platformProvisioningSnapshot(env.CONTROL_DB, resourceSlots(env));
}

async function retainedProvision(actor: ManageErpxPlatform, body: unknown, key: string) {
  const parsed = provisioningCommand.safeParse(body);
  if (!parsed.success)
    throw new ApiError(
      422,
      "VALIDATION_FAILED",
      parsed.error.issues[0]?.message ?? "Invalid provisioning request.",
    );
  const env = await runtimeEnv();
  const slot = resourceSlots(env).find((candidate) => candidate.id === parsed.data.holdingId);
  if (!slot)
    throw new ApiError(
      404,
      "RESOURCE_SLOT_NOT_FOUND",
      "This Holding resource slot is not configured in the operator registry.",
    );
  try {
    await holdingResources(slot.id);
  } catch {
    throw new ApiError(
      503,
      "RESOURCE_UNAVAILABLE",
      "The selected Holding database or private file namespace is unavailable.",
    );
  }
  return provisionHolding(
    actor,
    env.CONTROL_DB as PostgresDatabase,
    slot,
    {
      ...parsed.data,
      writeBinding: slot.write,
      readBinding: slot.read,
      filesBinding: slot.files,
    },
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
      throw new ApiError(422, "INVALID_JSON", "Provide a provisioning command.");
    }
    if (!body || typeof body !== "object" || Array.isArray(body))
      throw new ApiError(422, "COMMAND_REQUIRED", "Provide a provisioning command.");
    const data = await nativePlatformAdministrationOrRetained(
      async () => nativePlatformProvision(work, platform, await headers(), body, key),
      () => retainedProvision(platform, body, key),
    ).catch(failure);
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
    const data = await nativePlatformAdministrationOrRetained(
      async () => (await nativePlatformOperationsForPresentation(work, platform, await headers()))?.provisioning,
      retainedProvisioningSnapshot,
    ).catch(failure);
    return Response.json(canonicalDestinationLinks({ data }), {
      headers: { "Cache-Control": "private, no-store" },
    });
  } catch (error) {
    return errorResponse(error);
  }
}
