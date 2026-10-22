import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../../../../../../../packages/server-runtime/src/public/runtime";
import { errorResponse } from "@erpx/server-runtime/public/http";
import {
  readWorkCardPreferences,
  saveWorkCardPreferences,
  workCardPreferenceSchema,
} from "@erpx/server-work_projection/public/work-card-preferences";
import { getWorkSummary } from "@erpx/server-work_projection/public/work-summary";
import { serviceCommandBody } from "@erpx/server-support/public/service-http";
type Context = { params: Promise<{ holdingSlug: string }> };
export async function GET(_request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json(
      canonicalDestinationLinks({
        data: await readWorkCardPreferences(
          (await holdingResources(actor.holdingId)).write,
          actor,
        ),
      }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
export async function POST(request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug),
      { input, key } = await serviceCommandBody(request);
    const data = await saveWorkCardPreferences(
      (await holdingResources(actor.holdingId)).write,
      actor,
      workCardPreferenceSchema.parse(input),
      key,
      async () => (await getWorkSummary(actor)).cards.map((c) => c.id),
    );
    return Response.json(
      canonicalDestinationLinks({ data }),
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
