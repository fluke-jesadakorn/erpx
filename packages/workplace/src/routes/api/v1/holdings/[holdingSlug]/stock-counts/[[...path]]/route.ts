import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { z } from "zod";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { listCounts,countOptions,countDetail,saveCount,transitionCount,countSchema,countActionSchema } from "@erpx/server-inventory/public/inventory-counts";
import {initialPhysicalStockCountWorkspace,prepareInitialPhysicalStockCount,transitionInitialPhysicalStockCount,initialPhysicalStockCountSchema,initialPhysicalStockCountActionSchema,initialPhysicalStockCountFlow,assignInitialPhysicalStockCountFlow,initialPhysicalStockCountFlowSchema} from "@erpx/server-inventory/public/initial-physical-stock-count";
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
export async function GET(_request:Request,context:Context){try{const {holdingSlug,path:p=[]}=await context.params,actor=await requireWorkContext(holdingSlug);if(p[0]==="initial-physical"){
 if(p.length===3&&p[2]==="flow"){
 if(new URL(_request.url).search)throw new ApiError(422,"VALIDATION_FAILED","The record Flow does not accept a selection query.");
 return Response.json(canonicalDestinationLinks({data:await initialPhysicalStockCountFlow(actor,p[1])}),{headers:{"Cache-Control":"private, no-store"}});
 }
 if(p.length>2)throw new ApiError(404,"NOT_FOUND","Initial physical stock count route not found.");
 const url=new URL(_request.url),allowed=["warehouseId","productId","countDate"];
 if([...url.searchParams.keys()].some(key=>!allowed.includes(key)||url.searchParams.getAll(key).length!==1)||p.length===2&&url.search)throw new ApiError(422,"VALIDATION_FAILED","Use the selected initial physical count context.");
 const query=p.length===2?{countId:p[1]}:z.object({warehouseId:z.string().min(1).optional(),productId:z.string().min(1).optional(),countDate:z.iso.date().optional()}).strict().parse(Object.fromEntries(url.searchParams));
 return Response.json(canonicalDestinationLinks({data:await initialPhysicalStockCountWorkspace(actor,query)}),{headers:{"Cache-Control":"private, no-store"}});
 }if(p.length>1)throw new ApiError(404,"NOT_FOUND","Stock count route not found.");return Response.json(canonicalDestinationLinks({data:p.length?await countDetail(actor,p[0]):{counts:await listCounts(actor),options:await countOptions(actor)}}),{headers:{"Cache-Control":"private, no-store"}});}catch(e){if(e instanceof z.ZodError)return errorResponse(new ApiError(422,"VALIDATION_FAILED",e.issues[0]?.message??"Invalid input."));return errorResponse(e);}}
export async function POST(request:Request,context:Context){try{
 const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
 const {holdingSlug,path:p=[]}=await context.params,actor=await requireWorkContext(holdingSlug),key=request.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");const input=await request.json();let data;
 if(p[0]==="initial-physical"){
 if(p.length===1)data=await prepareInitialPhysicalStockCount(actor,initialPhysicalStockCountSchema.parse(input),key);
 else if(p.length===3&&p[2]==="flow")data=await assignInitialPhysicalStockCountFlow(actor,p[1],initialPhysicalStockCountFlowSchema.parse(input),key);
 else if(p.length===3&&["submit","reject","cancel","approve","post"].includes(p[2]))data=await transitionInitialPhysicalStockCount(actor,p[1],p[2] as "submit"|"reject"|"cancel"|"approve"|"post",initialPhysicalStockCountActionSchema.parse(input),key);
 else throw new ApiError(404,"NOT_FOUND","Initial physical stock count action not found.");
 }else if(p.length<=1)data=await saveCount(actor,p[0]??null,countSchema.parse(input),key);
 else if(p.length===2&&["submit","reject","cancel","approve","post"].includes(p[1]))data=await transitionCount(actor,p[0],p[1] as "submit"|"reject"|"cancel"|"approve"|"post",countActionSchema.parse(input),key);
 else throw new ApiError(404,"NOT_FOUND","Stock count action not found.");return Response.json(canonicalDestinationLinks({data}),{status:p.length===0||p.length===1&&p[0]==="initial-physical"?201:200,headers:{"Cache-Control":"private, no-store"}});
}catch(e){if(e instanceof z.ZodError)return errorResponse(new ApiError(422,"VALIDATION_FAILED",e.issues[0]?.message??"Invalid input."));if(e instanceof SyntaxError)return errorResponse(new ApiError(422,"INVALID_JSON","Use valid JSON."));return errorResponse(e);}}
