import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import {
  createNativeAgentAiGateway,
  nativeAgentAiGatewayAvailable,
} from "@erpx/server-agent/public/native-ai-gateway";
import { requireManageErpxContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";

const changeSchema = z.object({
  holdingId: z.string().min(3).max(24),
  enabled: z.boolean(),
  expectedUpdatedAt: z.number().int().min(0),
  reason: z.string().trim().min(3).max(1000),
});

const headers = { "Cache-Control": "private, no-store" };

/** Fail closed without the mounted native owner; there is no TypeScript fallback. */
function client() {
  const raw = process.env.ERPX_NATIVE_AGENT_AI_GATEWAY_CONFIG;
  if (!raw || !nativeAgentAiGatewayAvailable())
    throw new ApiError(
      503,
      "NATIVE_AGENT_AI_GATEWAY_UNAVAILABLE",
      "The native AI-gateway service is unavailable.",
    );
  return createNativeAgentAiGateway(raw);
}

export async function GET(request: Request) {
  try {
    const { platform } = await requireManageErpxContext();
    if (platform.role !== "platform_admin" && platform.role !== "platform_auditor")
      throw new ApiError(403, "FORBIDDEN", "Platform billing permission required.");
    const holdingId = new URL(request.url).searchParams.get("holdingId");
    if (!holdingId) throw new ApiError(422, "HOLDING_REQUIRED", "Select a Customer Holding.");
    const data = await client().policy(request.headers, holdingId);
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request) {
  try {
    const { platform } = await requireManageErpxContext();
    if (platform.role !== "platform_admin")
      throw new ApiError(403, "FORBIDDEN", "Only a Platform Admin can change the AI gateway policy.");
    const parsed = changeSchema.safeParse(await request.json());
    if (!parsed.success)
      throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid gateway policy request.");
    const data = await client().setEnabled(request.headers, parsed.data);
    return Response.json(canonicalDestinationLinks({ data }), { headers });
  } catch (error) { return errorResponse(error); }
}
