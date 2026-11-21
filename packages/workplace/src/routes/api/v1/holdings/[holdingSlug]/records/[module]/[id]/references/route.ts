import type { RouteContext } from "@erpx/contracts/next-routes";
import {canonicalDestinationLinks} from "@erpx/feature-work_projection/lib/destination-links";
import { requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { errorResponse,ApiError } from "@erpx/server-runtime/public/http";
import { addReference,addReferenceSchema } from "@erpx/server-records/public/records";
import { createNativeRecordContextClient, nativeRecordContextConfiguration, nativeRecordContextFailure } from "@erpx/server-records/public/native-record-context";
import { createNativeRecordRegistryClient, nativeRecordRegistryAvailable, nativeRecordRegistryConfiguration, nativeRecordRegistryFailure } from "@erpx/server-records/public/native-record-registry";

export async function POST(request:Request,context:RouteContext<"/api/v1/holdings/[holdingSlug]/records/[module]/[id]/references">){try{const {holdingSlug,module,id}=await context.params;const parsed=addReferenceSchema.safeParse(await request.json());if(!parsed.success)throw new ApiError(422,"VALIDATION_FAILED",parsed.error.issues[0]?.message??"Invalid reference.");const actor=await requireWorkContext(holdingSlug);if(nativeRecordRegistryAvailable()&&module==="support")return await addNativeReference(request,actor,holdingSlug,module,id,parsed.data);return Response.json(canonicalDestinationLinks({data:await addReference(actor,module,id,parsed.data,request.headers.get("Idempotency-Key")??undefined)}),{status:201});}catch(error){return errorResponse(error);}}
async function addNativeReference(request:Request,actor:Awaited<ReturnType<typeof requireWorkContext>>,holdingSlug:string,moduleId:string,recordId:string,input:{targetModule:string;targetId:string;kind:"dependsOn"|"blocks"|"billedBy"|"fulfills"|"authoredWith"}){
  if(!actor.authority||!actor.iam)throw new ApiError(409,"WORK_CONTEXT_CHANGED","Your work role changed. Reload and review before retrying.");
  const key=request.headers.get("Idempotency-Key");
  if(!key)throw new ApiError(422,"IDEMPOTENCY_KEY_REQUIRED","Provide a command key to add this reference.");
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
    const registry=createNativeRecordRegistryClient(nativeRecordRegistryConfiguration(raw));
    await registry.reference(request.headers,context,{holdingSlug,moduleId,recordId,targetModule:input.targetModule,targetId:input.targetId,kind:input.kind},key);
    const recordContext=await createNativeRecordContextClient(nativeRecordContextConfiguration(raw)).context(request.headers,context,{holdingSlug,moduleId,recordId});
    return Response.json(canonicalDestinationLinks({data:recordContext}),{status:201});
  } catch(error) {
    const mapped=nativeRecordRegistryFailure(error)??nativeRecordContextFailure(error);
    if(!mapped)throw error;
    throw new ApiError(mapped.status,mapped.code,mapped.message);
  }
}
