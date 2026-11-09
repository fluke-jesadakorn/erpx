import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ApiError,errorResponse } from "@erpx/server-runtime/public/http";
import { createRecord,createRecordSchema,listRecords,recordsPage } from "@erpx/server-records/public/records";
import { createNativeRecordRegistryClient, nativeRecordRegistryAvailable, nativeRecordRegistryConfiguration, nativeRecordRegistryFailure } from "@erpx/server-records/public/native-record-registry";

export const dynamic = "force-dynamic";

export async function GET(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]">){try{const {holdingSlug,module}=await context.params;const section=new URL(request.url).searchParams.get("section");if(!section)throw new ApiError(422,"SECTION_REQUIRED","Choose a module section.");const query=new URL(request.url).searchParams,actor=await requireWorkContext(holdingSlug);const data=nativeRecordRegistryAvailable()
  ? await readNativeRegistry(request,actor,holdingSlug,module,section,query)
  : query.get("paged")==="1"
    ? await recordsPage(actor,module,section,{q:query.get("q")??undefined,cursor:query.get("cursor")??undefined})
    : await listRecords(actor,module,section);return Response.json(canonicalDestinationLinks({data}),{headers:{"Cache-Control":"private, no-store"}});}catch(error){return errorResponse(error);}}
async function readNativeRegistry(request:Request,actor:Awaited<ReturnType<typeof requireWorkContext>>,holdingSlug:string,moduleId:string,sectionId:string,query:URLSearchParams){
  if(!actor.authority||!actor.iam)throw new ApiError(409,"WORK_CONTEXT_CHANGED","Your work role changed. Reload and review before retrying.");
  try {
    const raw=process.env.ERPX_NATIVE_RECORDS_CONFIG;
    if(raw===undefined)throw new ApiError(503,"NATIVE_RECORD_REGISTRY_UNAVAILABLE","The native Records registry reader is unavailable.");
    const client=createNativeRecordRegistryClient(nativeRecordRegistryConfiguration(raw));
    const scope=actor.scope;
    const branchSlug=scope.kind==="branch"||scope.kind==="department"?(scope.branchSlug??null):null;
    const departmentSlug=scope.kind==="department"?scope.departmentSlug:null;
    const context={userId:actor.userId,holdingId:actor.holdingId,holdingSlug,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,
      branchSlug,departmentSlug,
      assignmentId:actor.assignmentId,role:actor.role,assignmentScope:actor.assignmentScope,modules:actor.modules,
      authority:actor.authority,iam:{permissions:actor.iam.permissions}};
    if(query.get("paged")==="1")return await client.page(request.headers,context,{holdingSlug,moduleId,sectionId,q:query.get("q")||undefined,cursor:query.get("cursor")||undefined});
    return await client.list(request.headers,context,{holdingSlug,moduleId,sectionId});
  } catch(error) {
    const mapped=nativeRecordRegistryFailure(error);
    if(!mapped)throw error;
    throw new ApiError(mapped.status,mapped.code,mapped.message);
  }
}
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]">){try{const {holdingSlug,module}=await context.params;const body=await request.text();if(new TextEncoder().encode(body).length>64*1024)throw new ApiError(413,"REQUEST_TOO_LARGE","This record is too large.");let raw:unknown;try{raw=JSON.parse(body);}catch{throw new ApiError(422,"INVALID_JSON","Submit a valid record.");}const parsed=createRecordSchema.safeParse(raw);if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid record.");const actor=await requireWorkContext(holdingSlug);if(nativeRecordRegistryAvailable()&&module==="support")return await createNativeRecord(request,actor,holdingSlug,module,parsed.data);return Response.json(canonicalDestinationLinks({data:await createRecord(actor,module,parsed.data,request.headers.get("Idempotency-Key")??undefined)}),{status:201});}catch(error){return errorResponse(error);}}
async function createNativeRecord(request:Request,actor:Awaited<ReturnType<typeof requireWorkContext>>,holdingSlug:string,moduleId:string,input:{sectionId:string;title:string;description:string;fields:{key:string;label:string;type:string;value:string|number|boolean;options?:string[]}[]}){
  if(!actor.authority||!actor.iam)throw new ApiError(409,"WORK_CONTEXT_CHANGED","Your work role changed. Reload and review before retrying.");
  const key=request.headers.get("Idempotency-Key");
  if(!key)throw new ApiError(422,"IDEMPOTENCY_KEY_REQUIRED","Provide a command key to create this record.");
  try {
    const raw=process.env.ERPX_NATIVE_RECORDS_CONFIG;
    if(raw===undefined)throw new ApiError(503,"NATIVE_RECORD_REGISTRY_UNAVAILABLE","The native Records writer is unavailable.");
    const client=createNativeRecordRegistryClient(nativeRecordRegistryConfiguration(raw));
    const scope=actor.scope;
    const branchSlug=scope.kind==="branch"||scope.kind==="department"?(scope.branchSlug??null):null;
    const departmentSlug=scope.kind==="department"?scope.departmentSlug:null;
    const context={userId:actor.userId,holdingId:actor.holdingId,holdingSlug,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,
      branchSlug,departmentSlug,
      assignmentId:actor.assignmentId,role:actor.role,assignmentScope:actor.assignmentScope,modules:actor.modules,
      authority:actor.authority,iam:{permissions:actor.iam.permissions}};
    const record=await client.create(request.headers,context,{holdingSlug,moduleId,sectionId:input.sectionId,title:input.title,description:input.description,fields:input.fields},key);
    return Response.json(canonicalDestinationLinks({data:record}),{status:201});
  } catch(error) {
    const mapped=nativeRecordRegistryFailure(error);
    if(!mapped)throw error;
    throw new ApiError(mapped.status,mapped.code,mapped.message);
  }
}