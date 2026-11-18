import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import {z} from "zod";
import {requireManageErpxContext} from "@erpx/server-identity_access/public/authorization";
import {errorResponse,ApiError} from "@erpx/server-runtime/public/http";
import {nativePlatformAdminFailure,nativePlatformOperationsForPresentation,nativePlatformProvision} from "@erpx/feature-platform_administration/lib/native-platform-administration";
import {headers} from "next/headers";

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

function parseCommand(body: unknown) {
  const parsed = provisioningCommand.safeParse(body);
  if (!parsed.success)
    throw new ApiError(
      422,
      "VALIDATION_FAILED",
      parsed.error.issues[0]?.message ?? "Invalid provisioning request.",
    );
  return parsed.data;
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
    const command = parseCommand(body);
    const data = await nativePlatformProvision(
      work,
      platform,
      await headers(),
      command,
      key,
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
    const { provisioning } = await nativePlatformOperationsForPresentation(
      work,
      platform,
      await headers(),
    ).catch(failure);
    return Response.json(canonicalDestinationLinks({ data: provisioning }), {
      headers: { "Cache-Control": "private, no-store" },
    });
  } catch (error) {
    return errorResponse(error);
  }
}
