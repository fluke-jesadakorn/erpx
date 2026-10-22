import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import type { AccountQuickAccessArea } from "@erpx/server-identity_access/public/account-quick-access";
import {accountQuickAccessRequest} from '@erpx/server-identity_access/public/account-quick-access-request';
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { quickAccessPreferenceSchema } from "@erpx/server-work_projection/public/quick-access-preferences";
import { serviceCommandBody } from "@erpx/server-support/public/service-http";

export const dynamic = "force-dynamic";
type Context = { params: Promise<{ area: string }> };
const headers = { "Cache-Control": "private, no-store" };

function areaFrom(value: string): AccountQuickAccessArea {
  if (value === "my-access" || value === "security" || value === "work-role") return value;
  throw new ApiError(404, "NOT_FOUND", "Unknown account area.");
}

export async function GET(request: Request, context: Context) {
  try {
    const area = areaFrom((await context.params).area);
    return Response.json(canonicalDestinationLinks({ data: await accountQuickAccessRequest(request,area) }), { headers });
  } catch (error) {
    return errorResponse(error);
  }
}

export async function POST(request: Request, context: Context) {
  try {
    const area = areaFrom((await context.params).area);
    const { input, key } = await serviceCommandBody(request);
    const parsed = quickAccessPreferenceSchema.parse(input);
    const saved = await accountQuickAccessRequest(request,area,{...parsed,requestKey:key});
    return Response.json(canonicalDestinationLinks({ data: saved }), { headers });
  } catch (error) {
    return errorResponse(error);
  }
}
