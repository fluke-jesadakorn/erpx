import {serviceCountWriterOwner} from '@erpx/server-inventory/public/service-initial-physical-stock-count';
import {serviceCountCommand} from '../../../../../../../lib/service-count-native-command';
import {serviceCountRead} from '../../../../../../../lib/service-count-native-read';
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { z } from "zod";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { listCounts,countOptions,countDetail,countSchema,countActionSchema } from "@erpx/server-inventory/public/inventory-counts";
import { nativeStockCountSave, nativeStockCountTransition } from "@erpx/feature-inventory/lib/native-stock-count-command";
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
 }if(p.length>1)throw new ApiError(404,"NOT_FOUND","Stock count route not found.");const data=p.length?(await serviceCountWriterOwner(actor,p[0])==='service'?await serviceCountRead(_request.headers,actor,holdingSlug,{kind:'generic-detail',id:p[0]}):await countDetail(actor,p[0])):{counts:await listCounts(actor),options:await countOptions(actor)};return Response.json(canonicalDestinationLinks({data}),{headers:{"Cache-Control":"private, no-store"}});}catch(e){if(e instanceof z.ZodError)return errorResponse(new ApiError(422,"VALIDATION_FAILED",e.issues[0]?.message??"Invalid input."));return errorResponse(e);}}
export async function POST(request:Request,context:Context){try{
 const origin=request.headers.get("origin");if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");
 const {holdingSlug,path:p=[]}=await context.params,actor=await requireWorkContext(holdingSlug),key=request.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");if(p[0]!=="initial-physical" && p.length>=1 && (p.length===1 || p.length===2 && ["submit","reject","cancel","approve","post"].includes(p[1])) && await serviceCountWriterOwner(actor,p[0])==='service'){
  const route=p.length===1?{kind:'update' as const,id:p[0]}:{kind:'generic-action' as const,id:p[0],action:p[1] as 'submit'|'reject'|'cancel'|'approve'|'post'};
  const {data}=await serviceCountCommand(request,actor,holdingSlug,route,key);
  return Response.json(canonicalDestinationLinks({data}),{headers:{"Cache-Control":"private, no-store"}});
 }
 if(p[0]!=="initial-physical"&&p.length<=1){
  const parsedNative=countSchema.parse(await request.json());
  const native=await nativeStockCountSave(request,holdingSlug,p[0]??null,parsedNative);
  if(native)return native;
  throw new ApiError(503,"NATIVE_INVENTORY_COUNT_UNAVAILABLE","The native stock Count command is unavailable.");
 }
 const input=await request.json();let data;
 if(p[0]==="initial-physical"){
 if(p.length===1)data=await prepareInitialPhysicalStockCount(actor,initialPhysicalStockCountSchema.parse(input),key);
 else if(p.length===3&&p[2]==="flow")data=await assignInitialPhysicalStockCountFlow(actor,p[1],initialPhysicalStockCountFlowSchema.parse(input),key);
 else if(p.length===3&&["submit","reject","cancel","approve","post"].includes(p[2]))data=await transitionInitialPhysicalStockCount(actor,p[1],p[2] as "submit"|"reject"|"cancel"|"approve"|"post",initialPhysicalStockCountActionSchema.parse(input),key);
 else throw new ApiError(404,"NOT_FOUND","Initial physical stock count action not found.");
 }else if(p.length<=1)throw new ApiError(503,"NATIVE_INVENTORY_COUNT_UNAVAILABLE","The native stock Count command is unavailable.");
 else if(p.length===2&&["submit","reject","cancel","approve","post"].includes(p[1])){
  const action=p[1] as "submit"|"reject"|"cancel"|"approve"|"post",parsed=countActionSchema.parse(input);
  const native=await nativeStockCountTransition(request,holdingSlug,p[0],action,parsed);
  if(!native)throw new ApiError(503,"NATIVE_INVENTORY_COUNT_UNAVAILABLE","The native stock Count command is unavailable.");
  return native;
 }
 else throw new ApiError(404,"NOT_FOUND","Stock count action not found.");return Response.json(canonicalDestinationLinks({data}),{status:p.length===1&&p[0]==="initial-physical"?201:200,headers:{"Cache-Control":"private, no-store"}});
}catch(e){if(e instanceof z.ZodError)return errorResponse(new ApiError(422,"VALIDATION_FAILED",e.issues[0]?.message??"Invalid input."));if(e instanceof SyntaxError)return errorResponse(new ApiError(422,"INVALID_JSON","Use valid JSON."));return errorResponse(e);}}
