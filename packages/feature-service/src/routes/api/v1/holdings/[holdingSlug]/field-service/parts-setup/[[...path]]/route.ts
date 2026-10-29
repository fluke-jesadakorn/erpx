import {nativePartsSetup,nativePartsPrerequisites,NativePartsSetupError} from '@erpx/feature-field_service/public/native-parts-setup';
import {partsSetupContext} from '../../../../../../../../lib/parts-setup-context';
import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {requestPublicOrigin} from '@erpx/server-manage_erpx/public/public-origin';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
import {servicePartsPostingSetup,servicePartsStockWorkspace,initializeServicePartsPostingSetup,confirmExistingServicePartsPostingSetup,servicePartsPostingSetupSchema,servicePartsExistingPostingSetupSchema} from '@erpx/server-commerce/public/service-parts-posting-setup';
import {servicePartsPrerequisites,createServicePartsChart,createServicePartsAccount,createServicePartsPeriod,confirmServicePartsRate,servicePartsChartSchema,servicePartsAccountSchema,servicePartsPeriodSchema,servicePartsRateSchema} from '@erpx/server-finance/public/service-parts-prerequisites';
import {createServiceInitialPhysicalStockProduct,createServiceInitialPhysicalStockWarehouse,serviceInitialPhysicalStockProductSchema,serviceInitialPhysicalStockWarehouseSchema} from '@erpx/server-commerce/public/service-initial-physical-stock-masters';
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
const headers={'Cache-Control':'private, no-store'};
const missing=()=>new ApiError(404,'NOT_FOUND','Service parts preparation route not found.');
function failure(error:unknown){const mapped=error instanceof NativePartsSetupError?new ApiError(error.status,error.code,'Service parts preparation is unavailable.'):error instanceof SyntaxError?new ApiError(422,'INVALID_JSON','Use valid JSON.'):error,response=errorResponse(mapped);response.headers.set('Cache-Control',headers['Cache-Control']);return response;}
export async function GET(request:Request,context:Context){
 try{const {holdingSlug,path=[]}=await context.params;if(new URL(request.url).search||!(path.length===0||path.length===1&&['prerequisites','stock'].includes(path[0])))throw missing();const actor=await requireWorkContext(holdingSlug),data=path.length===0?await nativePartsSetup(request.headers,holdingSlug,partsSetupContext(actor),'GET')??await servicePartsPostingSetup(actor):path[0]==='stock'?await servicePartsStockWorkspace(actor):await nativePartsPrerequisites(request.headers,holdingSlug,partsSetupContext(actor),'read')??await servicePartsPrerequisites(actor);return Response.json({data},{headers});}catch(error){return failure(error);}
}
export async function POST(request:Request,context:Context){
 try{
  const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin for this action.');
  const {holdingSlug,path=[]}=await context.params,master=path.length===2&&path[0]==='masters'&&['products','warehouses'].includes(path[1]);if(new URL(request.url).search||!(path.length===0||master||path.length===1&&['existing','charts','accounts','periods','rates'].includes(path[0])))throw missing();
  const actor=await requireWorkContext(holdingSlug),key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const input=await request.json();let data;
  if(master)data=path[1]==='products'?await createServiceInitialPhysicalStockProduct(actor,serviceInitialPhysicalStockProductSchema.parse(input),key):await createServiceInitialPhysicalStockWarehouse(actor,serviceInitialPhysicalStockWarehouseSchema.parse(input),key);
  else if(path.length===0)data=await nativePartsSetup(request.headers,holdingSlug,partsSetupContext(actor),'POST',input,key)??await initializeServicePartsPostingSetup(actor,servicePartsPostingSetupSchema.parse(input),key);
  else if(path[0]==='existing')data=await nativePartsSetup(request.headers,holdingSlug,partsSetupContext(actor),'POST',input,key,true)??await confirmExistingServicePartsPostingSetup(actor,servicePartsExistingPostingSetupSchema.parse(input),key);
  else if(path[0]==='charts')data=await nativePartsPrerequisites(request.headers,holdingSlug,partsSetupContext(actor),'chart',input,key)??await createServicePartsChart(actor,servicePartsChartSchema.parse(input),key);
  else if(path[0]==='accounts')data=await nativePartsPrerequisites(request.headers,holdingSlug,partsSetupContext(actor),'account',input,key)??await createServicePartsAccount(actor,servicePartsAccountSchema.parse(input),key);
  else if(path[0]==='periods')data=await nativePartsPrerequisites(request.headers,holdingSlug,partsSetupContext(actor),'period',input,key)??await createServicePartsPeriod(actor,servicePartsPeriodSchema.parse(input),key);
  else data=await nativePartsPrerequisites(request.headers,holdingSlug,partsSetupContext(actor),'rate',input,key)??await confirmServicePartsRate(actor,servicePartsRateSchema.parse(input),key);
  return Response.json({data},{status:201,headers});
 }catch(error){return failure(error);}
}
