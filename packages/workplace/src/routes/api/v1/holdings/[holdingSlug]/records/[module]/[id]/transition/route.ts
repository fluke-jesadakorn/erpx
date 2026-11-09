import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requestPublicOrigin } from "@erpx/server-manage_erpx/public/public-origin";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { errorResponse, ApiError } from "@erpx/server-runtime/public/http";
import { transitionRecord, transitionSchema } from "@erpx/server-records/public/records";
import { createNativeRecordRegistryClient, nativeRecordRegistryAvailable, nativeRecordRegistryConfiguration, nativeRecordRegistryFailure } from "@erpx/server-records/public/native-record-registry";
export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]/transition">){
  try{if(request.headers.get("origin") && request.headers.get("origin")!==requestPublicOrigin(request))throw new ApiError(403,"ORIGIN_FORBIDDEN","Use the application origin.");const key=request.headers.get("Idempotency-Key");if(!key)throw new ApiError(422,"COMMAND_KEY_REQUIRED","Idempotency-Key is required.");const {holdingSlug,module,id}=await context.params;const parsed=transitionSchema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid transition.");
    const actor=await requireWorkContext(holdingSlug);
    if(nativeRecordRegistryAvailable()&&module==="support")return await transitionNativeRecord(request,actor,holdingSlug,module,id,parsed.data,key);
    return Response.json(canonicalDestinationLinks({data:await transitionRecord(actor,module,id,parsed.data,key)}));
  }catch(error){return errorResponse(error);}
}
async function transitionNativeRecord(request:Request,actor:Awaited<ReturnType<typeof requireWorkContext>>,holdingSlug:string,moduleId:string,recordId:string,input:{expectedVersion:number;action:"submit"|"approve"|"reject"|"complete"|"reopen";note:string},key:string){
  if(!actor.authority||!actor.iam)throw new ApiError(409,"WORK_CONTEXT_CHANGED","Your work role changed. Reload and review before retrying.");
  try {
    const raw=process.env.ERPX_NATIVE_RECORDS_CONFIG;
    if(raw===undefined)throw new ApiError(503,"NATIVE_RECORD_REGISTRY_UNAVAILABLE","The native Records writer is unavailable.");
    const scope=actor.scope;
    const branchSlug=scope.kind==="branch"||scope.kind==="department"?(scope.branchSlug??null):null;
    const departmentSlug=scope.kind==="department"?scope.departmentSlug:null;
    const context={userId:actor.userId,holdingId:actor.holdingId,holdingSlug,companyId:actor.companyId,branchId:actor.branchId,departmentId:actor.departmentId,
      branchSlug,departmentSlug,
      assignmentId:actor.assignmentId,role:actor.role,assignmentScope:actor.assignmentScope,modules:actor.modules,
      authority:actor.authority,iam:{permissions:actor.iam.permissions}};
    const client=createNativeRecordRegistryClient(nativeRecordRegistryConfiguration(raw));
    const record=await client.transition(request.headers,context,{holdingSlug,moduleId,recordId,expectedVersion:input.expectedVersion,action:input.action,note:input.note},key);
    return Response.json(canonicalDestinationLinks({data:record}));
  } catch(error) {
    const mapped=nativeRecordRegistryFailure(error);
    if(!mapped)throw error;
    throw new ApiError(mapped.status,mapped.code,mapped.message);
  }
}
