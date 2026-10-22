import {z} from 'zod';
import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {requestPublicOrigin} from '@erpx/server-manage_erpx/public/public-origin';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
import {createPosStartupChart,createPosStartupAccount,createPosStartupPeriod,confirmPosStartupRate,posStartupChartSchema,posStartupAccountSchema,posStartupPeriodSchema,posStartupRateSchema} from '@erpx/server-finance/public/pos-startup-prerequisites';
import {initializePosPostingSetup,posPostingSetupSchema} from '@erpx/server-commerce/public/pos-posting-setup';
import {createPosStartupProduct,createPosStartupWarehouse,posStartupProductSchema,posStartupWarehouseSchema} from '@erpx/server-commerce/public/pos-startup-masters';
export async function POST(request:Request,{params}:{params:Promise<{holdingSlug:string;path:string[]}>}){
 try{
  if(request.headers.get('origin')!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin.');
  const {holdingSlug,path}=await params;if(path.length!==1||!['charts','accounts','periods','rates','mapping','warehouses','products'].includes(path[0]))throw new ApiError(404,'NOT_FOUND','Unknown POS startup action.');
  const actor=await requireWorkContext(holdingSlug),key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');
  const input=await request.json(),data=path[0]==='charts'?await createPosStartupChart(actor,posStartupChartSchema.parse(input),key):path[0]==='accounts'?await createPosStartupAccount(actor,posStartupAccountSchema.parse(input),key):path[0]==='periods'?await createPosStartupPeriod(actor,posStartupPeriodSchema.parse(input),key):path[0]==='rates'?await confirmPosStartupRate(actor,posStartupRateSchema.parse(input),key):path[0]==='mapping'?await initializePosPostingSetup(actor,posPostingSetupSchema.parse(input),key):path[0]==='products'?await createPosStartupProduct(actor,posStartupProductSchema.parse(input),key):await createPosStartupWarehouse(actor,posStartupWarehouseSchema.parse(input),key);
  return Response.json({data},{status:201,headers:{'Cache-Control':'private, no-store'}});
 }catch(error){if(error instanceof z.ZodError)return errorResponse(new ApiError(422,'VALIDATION_FAILED',error.issues[0]?.message??'Invalid input.'));if(error instanceof SyntaxError)return errorResponse(new ApiError(422,'INVALID_JSON','Invalid JSON.'));return errorResponse(error);}
}
