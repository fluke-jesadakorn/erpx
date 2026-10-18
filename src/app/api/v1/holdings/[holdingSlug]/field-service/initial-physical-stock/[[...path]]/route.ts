import {requireWorkContext} from '@/server/modules/identity_access/public/authorization';
import {requestPublicOrigin} from '@/server/modules/manage_erpx/public/public-origin';
import {ApiError,errorResponse} from '@/server/modules/runtime/public/http';
import {serviceInitialPhysicalStockCountWorkspace,prepareServiceInitialPhysicalStockCount,transitionServiceInitialPhysicalStockCount,serviceInitialPhysicalStockCountFlow,assignServiceInitialPhysicalStockCountFlow,serviceInitialPhysicalStockCountSchema,serviceInitialPhysicalStockCountActionSchema,serviceInitialPhysicalStockCountSelectionSchema,serviceInitialPhysicalStockCountFlowSchema,type ServiceInitialPhysicalStockCountAction} from '@/server/modules/inventory/public/service-initial-physical-stock-count';
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
const headers={'Cache-Control':'private, no-store'};
const missing=()=>new ApiError(404,'NOT_FOUND','Service initial physical stock route not found.');
function failure(error:unknown){const mapped=error instanceof SyntaxError?new ApiError(422,'INVALID_JSON','Use valid JSON.'):error,response=errorResponse(mapped);response.headers.set('Cache-Control',headers['Cache-Control']);return response;}
const actions=['submit','approve','post','reject','cancel'] as const;
const isAction=(value:string):value is ServiceInitialPhysicalStockCountAction=>actions.some(action=>action===value);
export async function GET(request:Request,context:Context){
 try{
  const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug),url=new URL(request.url);if(path[0]==='masters')throw missing();
  if(path.length===2&&path[1]==='flow'){if(url.search)throw new ApiError(422,'VALIDATION_FAILED','The record Flow does not accept a selection query.');return Response.json({data:await serviceInitialPhysicalStockCountFlow(actor,path[0])},{headers});}
  if(path.length>1)throw missing();
  const allowed=['warehouseId','productId','countDate'];if(path.length===1&&url.search||[...url.searchParams.keys()].some(key=>!allowed.includes(key)||url.searchParams.getAll(key).length!==1))throw new ApiError(422,'VALIDATION_FAILED','Use one exact warehouse, product and count date selection.');
  const selection=serviceInitialPhysicalStockCountSelectionSchema.parse(path.length?{countId:path[0]}:Object.fromEntries(url.searchParams));return Response.json({data:await serviceInitialPhysicalStockCountWorkspace(actor,selection)},{headers});
 }catch(error){return failure(error);}
}
export async function POST(request:Request,context:Context){
 try{
  const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin for this action.');
  const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug);
  if(!(path.length===0||path[0]!=='masters'&&path.length===2&&(path[1]==='flow'||isAction(path[1]))))throw missing();
  const key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const input=await request.json();let data;
  if(path.length===0)data=await prepareServiceInitialPhysicalStockCount(actor,serviceInitialPhysicalStockCountSchema.parse(input),key);
  else if(path[1]==='flow')data=await assignServiceInitialPhysicalStockCountFlow(actor,path[0],serviceInitialPhysicalStockCountFlowSchema.parse(input),key);
  else if(isAction(path[1]))data=await transitionServiceInitialPhysicalStockCount(actor,path[0],path[1],serviceInitialPhysicalStockCountActionSchema.parse(input),key);
  else throw missing();return Response.json({data},{status:path.length===0?201:200,headers});
 }catch(error){return failure(error);}
}
