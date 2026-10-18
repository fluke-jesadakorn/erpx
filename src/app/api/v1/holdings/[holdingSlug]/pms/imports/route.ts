import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { z } from "zod";
import { requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { ApiError,errorResponse } from "@/server/modules/runtime/public/http";
import { requestPublicOrigin } from "@/server/modules/manage_erpx/public/public-origin";
import { isTransactionalSqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { previewPmsImport,approvePmsImport,applyPmsImport,pmsImportBatches } from "@/server/modules/pms/public/pms/import";
export const dynamic="force-dynamic";
type Context={params:Promise<{holdingSlug:string}>};
export async function GET(request:Request,{params}:Context){
  try{const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);
    const property=z.string().min(1).max(160).parse(new URL(request.url).searchParams.get("property"));
    return Response.json(canonicalDestinationLinks({data:await pmsImportBatches(write,actor,property)}),{headers:{"Cache-Control":"private, no-store"}});
  }catch(error){return errorResponse(error);}
}
export async function POST(request:Request,{params}:Context){
  try{
    if(request.headers.get("origin")&&request.headers.get("origin")!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use ERPX to save this change.");
    const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),{write}=await holdingResources(actor.holdingId);
    const key=request.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"IDEMPOTENCY_KEY_REQUIRED","An Idempotency-Key is required.");
    if(!isTransactionalSqlDatabase(write))throw new Error("POSTGRES_REQUIRED");
    const {action,input}=z.object({action:z.enum(["preview","approve","apply"]),input:z.unknown()}).strict().parse(await request.json());
    const result=action==="preview"?await previewPmsImport(write,actor,input,key):action==="approve"?await approvePmsImport(write,actor,input,key):await applyPmsImport(write,actor,input,key);
    return Response.json(canonicalDestinationLinks({data:result}));
  }catch(error){return errorResponse(error);}
}
