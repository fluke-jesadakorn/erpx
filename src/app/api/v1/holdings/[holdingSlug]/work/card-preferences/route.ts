import { requireWorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { errorResponse } from "@/server/http";
import {
  readWorkCardPreferences,
  saveWorkCardPreferences,
  workCardPreferenceSchema,
} from "@/server/work-card-preferences";
import { getWorkSummary } from "@/server/work-summary";
import { serviceCommandBody } from "@/server/service-http";
type Context = { params: Promise<{ holdingSlug: string }> };
export async function GET(_request: Request, context: Context) {
  try {
    const actor = await requireWorkContext((await context.params).holdingSlug);
    return Response.json(
      {
        data: await readWorkCardPreferences(
          (await holdingResources(actor.holdingId)).write,
          actor,
        ),
      },
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
      { data },
      { headers: { "Cache-Control": "private, no-store" } },
    );
  } catch (e) {
    return errorResponse(e);
  }
}
