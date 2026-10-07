import { z } from 'zod';
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { ApiError, errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { taxDocumentCommand, taxDocumentView } from "@/server/modules/finance/public/hospitality-tax-documents";
import {taxDocumentRequest} from "@/server/modules/finance/public/hospitality-tax-document-request";

type Context={params:Promise<{holdingSlug:string}>};
export async function GET(request:Request,context:Context) {
  try {
    const actor=await requireWorkContext((await context.params).holdingSlug);
    const invoiceId=z.string().uuid().parse(new URL(request.url).searchParams.get('invoiceId'));
    return Response.json({data:await taxDocumentView(actor,invoiceId)},{headers:{'Cache-Control':'private, no-store'}});
  } catch(error) { return errorResponse(error); }
}
export async function POST(request:Request,context:Context) {
  try {
    if (request.headers.get('origin')!==requestPublicOrigin(request)) throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
    const actor=await requireWorkContext((await context.params).holdingSlug);
    const {command,key}=await taxDocumentRequest(request);
    return Response.json({data:await taxDocumentCommand(actor,command,key)},{headers:{'Cache-Control':'private, no-store'}});
  } catch(error) { return errorResponse(error); }
}
