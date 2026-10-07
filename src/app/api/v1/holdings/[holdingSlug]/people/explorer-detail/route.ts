import { z } from 'zod';
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { peopleExplorerDetail } from "@/server/modules/people_payroll/public/people-explorer-detail";
import { measureReadRequest } from "@/server/modules/runtime/public/runtime/read-request";
const input=z.object({kind:z.enum(['company','branch','department','team','position','person','level','unassigned']),id:z.string().min(1).max(180),tab:z.string().min(1).max(40),archived:z.enum(['true','false']).optional(),changeId:z.string().min(1).max(200).optional(),changeState:z.string().max(20).optional(),changeQuery:z.string().max(120).optional(),changePage:z.string().max(12).optional()});
export const dynamic='force-dynamic';
export async function GET(request:Request,context:{params:Promise<{holdingSlug:string}>}) {
  return measureReadRequest('people_detail', async () => {
  try {
    const {holdingSlug}=await context.params,actor=await requireWorkContext(holdingSlug);
    const parsed=input.safeParse(Object.fromEntries(new URL(request.url).searchParams));
    if (!parsed.success) throw new ApiError(422,'INVALID_EXPLORER_SELECTION','Choose an organization item and section.');
    const {kind,id,tab,archived,...changeSearch}=parsed.data;
    return Response.json({data:await peopleExplorerDetail(actor,{kind,id},tab,archived==='true',changeSearch)},{headers:{'Cache-Control':'private, no-store'}});
  } catch (error) { return errorResponse(error); }
  });
}
