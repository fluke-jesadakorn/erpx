import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { runtimeEnv } from "@/server/modules/runtime/public/runtime";
import { stripeWebhook } from "@/server/modules/billing/public/beta-billing";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
export async function POST(request: Request) {
  try {
    if (Number(request.headers.get("content-length") ?? 0) > 256000)
      throw new ApiError(413, "EVENT_TOO_LARGE", "Event too large.");
    const reader = request.body?.getReader(),
      decoder = new TextDecoder();
    let raw = "",
      size = 0;
    if (reader)
      while (true) {
        const chunk = await reader.read();
        if (chunk.done) break;
        size += chunk.value.length;
        if (size > 256000) {
          await reader.cancel();
          throw new ApiError(413, "EVENT_TOO_LARGE", "Event too large.");
        }
        raw += decoder.decode(chunk.value, { stream: true });
      }
    raw += decoder.decode();
    return Response.json(
      canonicalDestinationLinks(await stripeWebhook(
        await runtimeEnv(),
        raw,
        request.headers.get("stripe-signature"),
      )),
    );
  } catch (e) {
    return errorResponse(e);
  }
}
