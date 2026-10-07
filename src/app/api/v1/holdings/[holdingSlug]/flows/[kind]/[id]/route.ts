import { z } from "zod";
import { flowKinds } from "@/lib/flow";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { requireFlowAccess } from "@/server/modules/workflow/public/flow-access";
import { assignFlowPrimary, resolveFlow } from "@/server/modules/workflow/public/flow-routing";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";

type Context = {params:Promise<{holdingSlug:string;kind:string;id:string}>};
const schema = z.object({stepKey:z.string().min(1).max(100),userId:z.string().min(1).max(180),expectedVersion:z.number().int().nonnegative(),recordVersion:z.number().int().positive(),reason:z.string().trim().min(3).max(2000),saveDefault:z.boolean().default(false)});
export async function GET(_request:Request, context:Context) {
  try {
    const p = await context.params, kind = z.enum(flowKinds).parse(p.kind), actor = await requireWorkContext(p.holdingSlug);
    const db = await requireFlowAccess(actor,kind,p.id);
    return Response.json({data:await resolveFlow(db,actor,kind,p.id)},{headers:{"Cache-Control":"private, no-store"}});
  } catch (error) { return errorResponse(error); }
}
export async function POST(request:Request, context:Context) {
  try {
    if (request.headers.get("origin") && request.headers.get("origin") !== requestPublicOrigin(request)) throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const p = await context.params, kind = z.enum(flowKinds).parse(p.kind), actor = await requireWorkContext(p.holdingSlug);
    const key = request.headers.get("Idempotency-Key"); if (!key) throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");
    const db = await requireFlowAccess(actor,kind,p.id);
    await assignFlowPrimary(db,actor,kind,p.id,schema.parse(await request.json()),key);
    return Response.json({data:await resolveFlow(db,actor,kind,p.id)},{headers:{"Cache-Control":"private, no-store"}});
  } catch (error) {
    if (error instanceof z.ZodError) return errorResponse(new ApiError(422,"VALIDATION_FAILED",error.issues[0]?.message ?? "Invalid input."));
    return errorResponse(error);
  }
}
