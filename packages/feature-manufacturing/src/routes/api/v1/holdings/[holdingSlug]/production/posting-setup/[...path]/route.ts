import {manufacturingExistingPostingSetup,confirmExistingManufacturingPostingSetup,manufacturingExistingPostingSetupSchema} from '@erpx/server-commerce/public/manufacturing-existing-posting-setup';
import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {requestPublicOrigin} from '@erpx/server-manage_erpx/public/public-origin';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
import {manufacturingPostingPrerequisites,createManufacturingPostingChart,createManufacturingPostingAccount,createManufacturingPostingPeriod,confirmManufacturingPostingRate,manufacturingPostingChartSchema,manufacturingPostingAccountSchema,manufacturingPostingPeriodSchema,manufacturingPostingRateSchema} from '@erpx/server-finance/public/manufacturing-posting-prerequisites';
import {manufacturingPostingProfiles,prepareManufacturingPostingProfile,decideManufacturingPostingProfile,manufacturingPostingProfileSchema,manufacturingPostingProfileDecisionSchema} from '@erpx/server-manufacturing/public/posting-profiles';
type Context={params:Promise<{holdingSlug:string;path:string[]}>};
const headers={'Cache-Control':'private, no-store'};
const missing=()=>new ApiError(404,'NOT_FOUND','Manufacturing setup action not found.');
const failure=(error:unknown)=>{const response=errorResponse(error);response.headers.set('Cache-Control',headers['Cache-Control']);return response;};
export async function GET(_request:Request,context:Context){
 try{const {holdingSlug,path}=await context.params,actor=await requireWorkContext(holdingSlug);if(path.length!==1||!['prerequisites','profiles','existing'].includes(path[0]))throw missing();return Response.json({data:path[0]==='existing'?await manufacturingExistingPostingSetup(actor):path[0]==='profiles'?await manufacturingPostingProfiles(actor):await manufacturingPostingPrerequisites(actor)},{headers});}catch(error){return failure(error);}
}
export async function POST(request:Request,context:Context){
 try{
  const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin for this action.');
  const {holdingSlug,path}=await context.params,actor=await requireWorkContext(holdingSlug);if(!(path.length===1&&['charts','accounts','periods','rates','profiles'].includes(path[0])||path.length===2&&path[0]==='existing'&&path[1]==='confirm'||path.length===3&&path[0]==='profiles'&&path[2]==='decide'))throw missing();
  const key=request.headers.get('Idempotency-Key');if(!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const input=await request.json();let data;
  if(path[0]==='existing')data=await confirmExistingManufacturingPostingSetup(actor,manufacturingExistingPostingSetupSchema.parse(input),key);
  else if(path[0]==='charts')data=await createManufacturingPostingChart(actor,manufacturingPostingChartSchema.parse(input),key);
  else if(path[0]==='accounts')data=await createManufacturingPostingAccount(actor,manufacturingPostingAccountSchema.parse(input),key);
  else if(path[0]==='periods')data=await createManufacturingPostingPeriod(actor,manufacturingPostingPeriodSchema.parse(input),key);
  else if(path[0]==='rates')data=await confirmManufacturingPostingRate(actor,manufacturingPostingRateSchema.parse(input),key);
  else if(path.length===1)data=await prepareManufacturingPostingProfile(actor,manufacturingPostingProfileSchema.parse(input),key);
  else data=await decideManufacturingPostingProfile(actor,path[1],manufacturingPostingProfileDecisionSchema.parse(input),key);
  return Response.json({data},{status:path.length===3?200:201,headers});
 }catch(error){return failure(error);}
}
