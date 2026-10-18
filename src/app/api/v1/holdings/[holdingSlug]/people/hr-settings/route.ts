import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { z } from "zod";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { peopleHrSettingsData, peopleHrTopics, peopleMoveWorkImpact, peopleNewPersonData, peoplePositionWorkingTimeData, peopleWorkingTimeData } from "@/server/modules/people_payroll/public/people-hr-settings";

export const dynamic = "force-dynamic";
const query = z.object({topic:z.enum(peopleHrTopics).optional(),editor:z.literal("person").optional(),engagement:z.string().min(1).max(180).optional(),position:z.string().min(1).max(180).optional(),impact:z.string().min(1).max(18099).transform(value=>value.split(",")).pipe(z.array(z.string().min(1).max(180)).min(1).max(100)).optional()}).refine(value => Number(!!value.topic) + Number(!!value.engagement) + Number(!!value.position) + Number(!!value.impact) + Number(!!value.editor) <= 1);

export async function GET(request: Request, {params}: {params:Promise<{holdingSlug:string}>}) {
  try {
    const input = query.safeParse(Object.fromEntries(new URL(request.url).searchParams));
    if (!input.success) throw new ApiError(422, "INVALID_HR_SETTING", "Choose one HR settings topic or staff record.");
    const actor = await requireWorkContext((await params).holdingSlug);
    const data = input.data.editor ? await peopleNewPersonData(actor) : input.data.impact ? await peopleMoveWorkImpact(actor,input.data.impact) : input.data.engagement ? await peopleWorkingTimeData(actor, input.data.engagement) : input.data.position ? await peoplePositionWorkingTimeData(actor, input.data.position) : await peopleHrSettingsData(actor, input.data.topic);
    return Response.json(canonicalDestinationLinks({data}), {headers:{"Cache-Control":"private, no-store"}});
  } catch (error) { return errorResponse(error); }
}
