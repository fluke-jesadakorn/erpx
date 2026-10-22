import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { z } from "zod";
import { readAiGatewayPolicy, setAiPlatformKeyEnabled } from "@erpx/server-agent/public/ai-gateway";
import { requireManageErpxContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { activeHoldingBindings, holdingResources } from "../../../../../../../../../packages/server-runtime/src/public/runtime";

const changeSchema = z.object({
  holdingId: z.string().min(3).max(24),
  enabled: z.boolean(),
  expectedUpdatedAt: z.number().int().min(0),
  reason: z.string().trim().min(3).max(1000),
});

async function holdingDatabase(holdingId: string) {
  const binding = (await activeHoldingBindings()).find((item) => item.id === holdingId && item.id !== "h003");
  if (!binding) throw new ApiError(404, "NOT_FOUND", "Customer Holding not found.");
  return (await holdingResources(binding.id)).write;
}

export async function GET(request: Request) {
  try {
    const { platform } = await requireManageErpxContext();
    if (platform.role !== "platform_admin" && platform.role !== "platform_auditor")
      throw new ApiError(403, "FORBIDDEN", "Platform billing permission required.");
    const holdingId = new URL(request.url).searchParams.get("holdingId");
    if (!holdingId) throw new ApiError(422, "HOLDING_REQUIRED", "Select a Customer Holding.");
    return Response.json(canonicalDestinationLinks({ data: await readAiGatewayPolicy(await holdingDatabase(holdingId)) }),
      { headers: { "Cache-Control": "private, no-store" } });
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
    const db = await holdingDatabase(parsed.data.holdingId);
    const previous = await readAiGatewayPolicy(db);
    if (previous.updatedAt !== parsed.data.expectedUpdatedAt)
      throw new ApiError(409, "AI_GATEWAY_POLICY_CHANGED", "The AI gateway policy changed. Refresh before saving.");
    if (previous.platformEnabled === parsed.data.enabled)
      throw new ApiError(409, "AI_GATEWAY_POLICY_UNCHANGED", "The AI gateway is already in that state.");
    await setAiPlatformKeyEnabled(db, {
      enabled: parsed.data.enabled, actorId: platform.userId, reason: parsed.data.reason,
    });
    return Response.json(canonicalDestinationLinks({ data: await readAiGatewayPolicy(db) }),
      { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
