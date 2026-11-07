import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {z} from "zod";
import {requireManageErpxContext} from "@erpx/server-identity_access/public/authorization";
import {errorResponse,ApiError} from "@erpx/server-runtime/public/http";
import {activeHoldingBindings,holdingResources} from "../../../../../../../../../packages/server-runtime/src/public/runtime";
import {recoverDeadLetterJob} from "@erpx/server-platform_administration/public/platform-operations";
import {nativePlatformAdminFailure,nativePlatformAdministrationOrRetained,nativePlatformRecover} from "@erpx/feature-platform_administration/lib/native-platform-administration";
import {headers} from "next/headers";

type ManageErpxPlatform = Awaited<ReturnType<typeof requireManageErpxContext>>["platform"];

const recoveryCommand = z.object({
  holdingId: z.string().min(3).max(24),
  eventId: z.string().min(2).max(180),
  expectedAttempts: z.number().int().min(1),
  expectedLastError: z.string().max(64).nullable(),
  reason: z.string().trim().min(3).max(1000),
});

function failure(error: unknown): never {
  const mapped = nativePlatformAdminFailure(error);
  if (mapped) throw new ApiError(mapped.status, mapped.code, mapped.message);
  throw error;
}

async function retainedRecovery(actor: ManageErpxPlatform, body: unknown, key: string) {
  const parsed = recoveryCommand.safeParse(body);
  if (!parsed.success)
    throw new ApiError(
      422,
      "VALIDATION_FAILED",
      parsed.error.issues[0]?.message ?? "Invalid recovery request.",
    );
  const binding = (await activeHoldingBindings()).find(
    (item) => item.id === parsed.data.holdingId,
  );
  if (!binding) throw new ApiError(404, "NOT_FOUND", "Holding not found.");
  return recoverDeadLetterJob(
    actor,
    (await holdingResources(binding.id)).write,
    binding.id,
    parsed.data,
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
      throw new ApiError(422, "INVALID_JSON", "Provide a failed job recovery command.");
    }
    if (!body || typeof body !== "object" || Array.isArray(body))
      throw new ApiError(422, "COMMAND_REQUIRED", "Provide a failed job recovery command.");
    const data = await nativePlatformAdministrationOrRetained(
      async () => nativePlatformRecover(work, platform, await headers(), body, key),
      () => retainedRecovery(platform, body, key),
    ).catch(failure);
    return Response.json(canonicalDestinationLinks({ data }), {
      headers: { "Cache-Control": "private, no-store" },
    });
  } catch (error) {
    return errorResponse(error);
  }
}
