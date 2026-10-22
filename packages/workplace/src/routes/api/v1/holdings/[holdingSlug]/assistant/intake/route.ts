import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { guidedIntakeInputSchema } from "@erpx/feature-shared/lib/ai/guided-intake";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources, runtimeEnv } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { guidedIntakeOptions, prepareGuidedIntake } from "@erpx/server-operations/public/guided-intake";
import { generateThaiHotelMockAi, isThaiHotelAiMockEnabled } from "@erpx/server-runtime/public/simulation-only-adapters";

type Context = { params: Promise<{ holdingSlug: string }> };
export async function GET(_request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const { write } = await holdingResources(actor.holdingId);
    return Response.json(canonicalDestinationLinks({ data: await guidedIntakeOptions(write,actor,await runtimeEnv()) }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}

export async function POST(request: Request, context: Context) {
  try {
    if (request.headers.get("origin") !== requestPublicOrigin(request))
      throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
    const id = request.headers.get("Idempotency-Key");
    if (!id || !/^[a-zA-Z0-9_-]{8,100}$/.test(id))
      throw new ApiError(422,"COMMAND_KEY_REQUIRED","Use a valid Idempotency-Key.");
    const actor = await requireWorkContext((await context.params).holdingSlug);
    const { write } = await holdingResources(actor.holdingId);
    const reader = request.body?.getReader();
    if (!reader) throw new ApiError(422,"INVALID_JSON","A message is required.");
    let body = "", bytes = 0;
    const decoder = new TextDecoder();
    while (true) {
      const part = await reader.read();
      if (part.done) break;
      bytes += part.value.byteLength;
      if (bytes > 16000) { await reader.cancel(); throw new ApiError(413,"MESSAGE_TOO_LARGE","Shorten this message."); }
      body += decoder.decode(part.value,{stream:true});
    }
    let raw: unknown;
    try { raw = JSON.parse(body + decoder.decode()); }
    catch { throw new ApiError(422,"INVALID_JSON","Use valid JSON."); }
    const input = guidedIntakeInputSchema.parse(raw);
    const data = await prepareGuidedIntake(write,actor,await runtimeEnv(),input,id,
      isThaiHotelAiMockEnabled(actor.holdingSlug) ? generateThaiHotelMockAi : undefined);
    return Response.json(canonicalDestinationLinks({ data }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) { return errorResponse(error); }
}
