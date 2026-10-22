import {requireWorkContext} from '@erpx/server-identity_access/public/authorization';
import {requestPublicOrigin} from '@erpx/server-manage_erpx/public/public-origin';
import {ApiError,errorResponse} from '@erpx/server-runtime/public/http';
import {projectWorkerStartupWorkspace,projectWorkerStartupAccounts,projectWorkerStartupPeople,prepareProjectWorkerStartup,previewProjectWorkerPlacement,submitProjectWorkerPlacement,cancelProjectWorkerPlacement,projectWorkerSelectionSchema,projectWorkerAccountsSchema,projectWorkerPeopleSchema,projectWorkerPrepareSchema,projectWorkerPlacementSchema,projectWorkerPlacementSubmitSchema,projectWorkerPlacementCancelSchema} from '@erpx/server-people_payroll/public/project-worker-startup';
type Context={params:Promise<{holdingSlug:string;path?:string[]}>};
const headers={'Cache-Control':'private, no-store'},missing=()=>new ApiError(404,'NOT_FOUND','Project worker setup route not found.');
function failure(error:unknown){const response=errorResponse(error instanceof SyntaxError?new ApiError(422,'INVALID_JSON','Use valid JSON.'):error);response.headers.set('Cache-Control',headers['Cache-Control']);return response;}
export async function GET(request:Request,context:Context){try{
 const {holdingSlug,path=[]}=await context.params,url=new URL(request.url),accounts=path.length===1&&path[0]==='accounts',people=path.length===1&&path[0]==='people';if(!(path.length===0||accounts||people))throw missing();const keys=accounts||people?['query']:['accountId','personId'];if([...url.searchParams.keys()].some(key=>!keys.includes(key)||url.searchParams.getAll(key).length!==1))throw new ApiError(422,'VALIDATION_FAILED','Use one exact Account or Person selection.');
 const actor=await requireWorkContext(holdingSlug),raw=Object.fromEntries(url.searchParams),data=people?await projectWorkerStartupPeople(actor,projectWorkerPeopleSchema.parse(raw)):accounts?await projectWorkerStartupAccounts(actor,projectWorkerAccountsSchema.parse(raw)):await projectWorkerStartupWorkspace(actor,projectWorkerSelectionSchema.parse(raw));return Response.json({data},{headers});
}catch(error){return failure(error);}}
export async function POST(request:Request,context:Context){try{
 const origin=request.headers.get('origin');if(origin&&origin!==requestPublicOrigin(request))throw new ApiError(403,'ORIGIN_FORBIDDEN','Use the application origin for this action.');const {holdingSlug,path=[]}=await context.params,prepare=path.length===1&&path[0]==='prepare',preview=path.length===3&&path[1]==='placement'&&path[2]==='preview',submit=path.length===3&&path[1]==='placement'&&path[2]==='submit',cancel=path.length===4&&path[1]==='placements'&&path[3]==='cancel';
 if(new URL(request.url).search||!(prepare||preview||submit||cancel)||!prepare&&['accounts','people','prepare'].includes(path[0]))throw missing();const actor=await requireWorkContext(holdingSlug),key=request.headers.get('Idempotency-Key');if(!preview&&!key)throw new ApiError(422,'COMMAND_KEY_REQUIRED','Idempotency-Key is required.');const raw=await request.json();let data;
 if(prepare)data=await prepareProjectWorkerStartup(actor,projectWorkerPrepareSchema.parse(raw),key!);
 else if(preview)data=await previewProjectWorkerPlacement(actor,path[0],projectWorkerPlacementSchema.parse(raw));
 else if(submit)data=await submitProjectWorkerPlacement(actor,path[0],projectWorkerPlacementSubmitSchema.parse(raw),key!);
 else data=await cancelProjectWorkerPlacement(actor,path[0],path[2],projectWorkerPlacementCancelSchema.parse(raw),key!);
 return Response.json({data},{status:prepare||submit?201:200,headers});
}catch(error){return failure(error);}}
