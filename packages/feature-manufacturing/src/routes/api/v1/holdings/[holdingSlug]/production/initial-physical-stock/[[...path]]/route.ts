import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {requestPublicOrigin} from '@erpx/server-manage_erpx/public/public-origin';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
import {manufacturingInitialPhysicalStockCountWorkspace,prepareManufacturingInitialPhysicalStockCount,transitionManufacturingInitialPhysicalStockCount,manufacturingInitialPhysicalStockCountFlow,assignManufacturingInitialPhysicalStockCountFlow,manufacturingInitialPhysicalStockCountSchema,manufacturingInitialPhysicalStockCountActionSchema,manufacturingInitialPhysicalStockCountSelectionSchema,manufacturingInitialPhysicalStockCountFlowSchema,type ManufacturingInitialPhysicalStockCountAction} from '@erpx/server-inventory/public/manufacturing-initial-physical-stock-count';
import {createManufacturingInitialPhysicalStockProduct,createManufacturingInitialPhysicalStockWarehouse,manufacturingInitialPhysicalStockProductSchema,manufacturingInitialPhysicalStockWarehouseSchema} from '@erpx/server-commerce/public/manufacturing-initial-physical-stock-masters';
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
const headers={'Cache-Control':'private, no-store'};
const missing=()=>new ApiError(404,'NOT_FOUND','Manufacturing initial physical stock route not found.');
function failure(error:unknown){const mapped=error instanceof SyntaxError?new ApiError(422,'INVALID_JSON','Use valid JSON.'):error,response=errorResponse(mapped);response.headers.set('Cache-Control',headers['Cache-Control']);return response;}
const actions=['submit','approve','post','reject','cancel'] as const;
const isAction=(value:string):value is ManufacturingInitialPhysicalStockCountAction=>actions.some(action=>action===value);
export async function GET(request:Request,context:Context){
 try{
  const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug),url=new URL(request.url);if(path[0]==='masters')throw missing();
  if(path.length===2&&path[1]==='flow'){if(url.search)throw new ApiError(422,'VALIDATION_FAILED','The record Flow does not accept a selection query.');return Response.json({data:await manufacturingInitialPhysicalStockCountFlow(actor,path[0])},{headers});}
  if(path.length>1)throw missing();
  const allowed=['warehouseId','productId','countDate'];if(path.length===1&&url.search||[...url.searchParams.keys()].some(key=>!allowed.includes(key)||url.searchParams.getAll(key).length!==1))throw new ApiError(422,'VALIDATION_FAILED','Use one exact warehouse, product and count date selection.');
  const selection=manufacturingInitialPhysicalStockCountSelectionSchema.parse(path.length?{countId:path[0]}:Object.fromEntries(url.searchParams));return Response.json({data:await manufacturingInitialPhysicalStockCountWorkspace(actor,selection)},{headers});
 }catch(error){return failure(error);}
}
export async function POST(request:Request,context:Context){
 try{
  const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin for this action.');
  const {holdingSlug,path=[]}=await context.params,actor=await requireWorkContext(holdingSlug),master=path.length===2&&path[0]==='masters'&&['products','warehouses'].includes(path[1]);
  if(!(path.length===0||master||path[0]!=='masters'&&path.length===2&&(path[1]==='flow'||isAction(path[1]))))throw missing();
  const key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const input=await request.json();let data;
  if(master)data=path[1]==='products'?await createManufacturingInitialPhysicalStockProduct(actor,manufacturingInitialPhysicalStockProductSchema.parse(input),key):await createManufacturingInitialPhysicalStockWarehouse(actor,manufacturingInitialPhysicalStockWarehouseSchema.parse(input),key);
  else if(path.length===0)data=await prepareManufacturingInitialPhysicalStockCount(actor,manufacturingInitialPhysicalStockCountSchema.parse(input),key);
  else if(path[1]==='flow')data=await assignManufacturingInitialPhysicalStockCountFlow(actor,path[0],manufacturingInitialPhysicalStockCountFlowSchema.parse(input),key);
  else if(isAction(path[1]))data=await transitionManufacturingInitialPhysicalStockCount(actor,path[0],path[1],manufacturingInitialPhysicalStockCountActionSchema.parse(input),key);
  else throw missing();return Response.json({data},{status:path.length===0||master?201:200,headers});
 }catch(error){return failure(error);}
}
