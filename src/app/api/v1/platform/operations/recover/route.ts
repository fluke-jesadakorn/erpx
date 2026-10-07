import { z } from "zod";
import { requireManageErpxContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse, ApiError } from "@/server/modules/runtime/public/http";
import { activeHoldingBindings, holdingResources } from "@/server/modules/runtime/public/runtime";
import { recoverDeadLetterJob } from "@/server/modules/platform_administration/public/platform-operations";

const schema = z.object({
  holdingId: z.string().min(3).max(24),
  eventId: z.string().min(2).max(180),
  expectedAttempts: z.number().int().min(1),
  expectedLastError: z.string().max(64).nullable(),
  reason: z.string().trim().min(3).max(1000),
});

export async function POST(request: Request) {
  try {
    const { platform: actor } = await requireManageErpxContext();
    const key = request.headers.get("Idempotency-Key") ?? "";
    const parsed = schema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid recovery request.");
    const binding = (await activeHoldingBindings()).find((item) => item.id === parsed.data.holdingId);
    if (!binding) throw new ApiError(404, "NOT_FOUND", "Holding not found.");
    const result = await recoverDeadLetterJob(
      actor,
      (await holdingResources(binding.id)).write,
      binding.id,
      parsed.data,
      key,
    );
    return Response.json({ data: result }, { headers: { "Cache-Control": "no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}
