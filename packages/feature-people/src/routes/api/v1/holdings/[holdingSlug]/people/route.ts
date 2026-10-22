import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { errorResponse } from "@erpx/server-runtime/public/http";
import { listPeople } from "@erpx/server-people_payroll/public/people-payroll";

export const dynamic = "force-dynamic";

export async function GET(request: Request, { params }: { params: Promise<{ holdingSlug: string }> }) {
  try {
    const { holdingSlug } = await params;
    return Response.json(canonicalDestinationLinks({ data: await listPeople(await requireWorkContext(holdingSlug),{levelId:new URL(request.url).searchParams.get("levelId")??undefined,departmentId:new URL(request.url).searchParams.get("departmentId")??undefined}) }), { headers: { "Cache-Control": "private, no-store" } });
  } catch (error) {
    return errorResponse(error);
  }
}
