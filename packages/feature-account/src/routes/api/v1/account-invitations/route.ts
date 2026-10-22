import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { z } from "zod";
import { acceptInvitation,acceptInvitationSchema,inspectInvitation } from "@erpx/server-identity_access/public/account-access";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
export async function GET(request:Request){try{return Response.json(canonicalDestinationLinks({data:await inspectInvitation(new URL(request.url).searchParams.get("token")??"")}),{headers:{"Cache-Control":"no-store"}});}catch(e){return errorResponse(e);}}
export async function POST(request:Request){try{if(request.headers.get("origin")&&request.headers.get("origin")!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");return Response.json(canonicalDestinationLinks({data:await acceptInvitation(acceptInvitationSchema.parse(await request.json()))}));}catch(e){if(e instanceof z.ZodError)return errorResponse(new ApiError(422,"VALIDATION_FAILED",e.issues[0]?.message??"Invalid invitation."));return errorResponse(e);}}
