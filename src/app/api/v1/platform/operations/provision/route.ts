import { z } from "zod";
import { requireManageErpxContext } from "@/server/modules/identity_access/public/authorization";
import { errorResponse, ApiError } from "@/server/modules/runtime/public/http";
import { resourceSlots } from "@/lib/holding-registry";
import { holdingResources, runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { provisionHolding, platformProvisioningSnapshot } from "@/server/modules/platform_administration/public/platform-operations";

const schema = z.object({
  holdingId: z.string().regex(/^[a-z0-9]{3,24}$/),
  slug: z.string().regex(/^[a-z][a-z0-9-]{2,39}$/),
  displayName: z.string().trim().min(2).max(120),
  reason: z.string().trim().min(10).max(1000),
});

export async function POST(request: Request) {
  try {
    const { platform: actor } = await requireManageErpxContext();
    const key = request.headers.get("Idempotency-Key") ?? "";
    const parsed = schema.safeParse(await request.json());
    if (!parsed.success) throw new ApiError(422, "VALIDATION_FAILED", parsed.error.issues[0]?.message ?? "Invalid provisioning request.");
    const env = await runtimeEnv();
    const slot = resourceSlots(env).find((candidate) => candidate.id === parsed.data.holdingId);
    if (!slot) throw new ApiError(404, "RESOURCE_SLOT_NOT_FOUND", "This Holding resource slot is not configured in the operator registry.");
    try {
      await holdingResources(slot.id);
    } catch {
      throw new ApiError(503, "RESOURCE_UNAVAILABLE", "The selected Holding database or private file namespace is unavailable.");
    }
    const result = await provisionHolding(
      actor,
      env.CONTROL_DB as import("@/server/modules/runtime/public/runtime/postgres").PostgresDatabase,
      slot,
      { ...parsed.data, writeBinding: slot.write, readBinding: slot.read, filesBinding: slot.files },
      key,
    );
    return Response.json({ data: result }, { headers: { "Cache-Control": "no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}

export async function GET() {
  try {
    await requireManageErpxContext();
    const env=await runtimeEnv();
    return Response.json({data:await platformProvisioningSnapshot(env.CONTROL_DB,resourceSlots(env))},{headers:{"Cache-Control":"private, no-store"}});
  } catch(error) {return errorResponse(error);}
}
