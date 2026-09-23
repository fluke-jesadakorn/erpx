import { z } from 'zod';
import { requireWorkContext } from '@/server/authorization';
import { ApiError, errorResponse } from '@/server/http';
import { requestPublicOrigin } from '@/server/public-origin';
import { taxDocumentCommand, taxDocumentCommandSchema, taxDocumentView } from '@/server/hospitality-tax-documents';

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
    const key=request.headers.get('Idempotency-Key');
    if (!key || !/^[a-zA-Z0-9_-]{8,100}$/.test(key)) throw new ApiError(422,'COMMAND_KEY_REQUIRED','Use a valid Idempotency-Key.');
    const reader=request.body?.getReader();
    if (!reader) throw new ApiError(422,'INVALID_JSON','Document details are required.');
    let bytes=0,body='';const decoder=new TextDecoder();
    while (true) {
      const part=await reader.read();if(part.done)break;
      bytes+=part.value.byteLength;
      if (bytes>4096) {await reader.cancel();throw new ApiError(413,'DOCUMENT_TOO_LARGE','Shorten the document details.');}
      body+=decoder.decode(part.value,{stream:true});
    }
    const actor=await requireWorkContext((await context.params).holdingSlug);
    let parsed:unknown;
    try {parsed=JSON.parse(body+decoder.decode());}
    catch {throw new ApiError(422,'INVALID_JSON','Use valid JSON.');}
    const command=taxDocumentCommandSchema.parse(parsed);
    return Response.json({data:await taxDocumentCommand(actor,command,key)},{headers:{'Cache-Control':'private, no-store'}});
  } catch(error) { return errorResponse(error); }
}
