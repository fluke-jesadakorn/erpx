import {supportTicketPortalProjectionRequest} from '@erpx/server-support/public/ticket-portal';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
export const dynamic='force-dynamic';
export async function GET(request:Request,context:{params:Promise<{holdingSlug:string}>}){try{if(new URL(request.url).search)throw new ApiError(422,'VALIDATION_FAILED','Use the verified ticket list without selectors.');return Response.json({data:await supportTicketPortalProjectionRequest((await context.params).holdingSlug)},{headers:{'Cache-Control':'private, no-store'}});}catch(error){const response=errorResponse(error);response.headers.set('Cache-Control','private, no-store');return response;}}
