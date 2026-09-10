import { requireWorkContext } from "@/server/authorization";
import { errorResponse } from "@/server/http";
import { listPeople } from "@/server/people-payroll";

export const dynamic = "force-dynamic";

export async function GET(request: Request, { params }: { params: Promise<{ holdingSlug: string }> }) {
  try {
    const { holdingSlug } = await params;
    return Response.json({ data: await listPeople(await requireWorkContext(holdingSlug),{levelId:new URL(request.url).searchParams.get("levelId")??undefined,departmentId:new URL(request.url).searchParams.get("departmentId")??undefined}) });
  } catch (error) {
    return errorResponse(error);
  }
}
