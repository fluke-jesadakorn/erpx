import {canonicalDestinationLinks} from "@/lib/work_projection/destination-links";
import { ZodError } from "zod";
export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
    public details?: {currentVersion:number},
  ) {
    super(message);
  }
}

export function errorResponse(error: unknown) {
  if(error instanceof Error && error.message==='PMS_OWNER_WORKFLOW_REQUIRED') return Response.json(canonicalDestinationLinks({error:{code:'PMS_OWNER_WORKFLOW_REQUIRED',message:'Edit this item through PMS so the guest account and ledger stay aligned.'}}),{status:409});
  if(error instanceof Error&&error.message==='QUALITY_HOLD')return Response.json(canonicalDestinationLinks({error:{code:'QUALITY_HOLD',message:'This product is quarantined in this warehouse. Quality inspection and independent release are required before reservation or stock issue.'}}),{status:409});
  if(error instanceof Error&&error.message==='STOCK_RESERVED')return Response.json(canonicalDestinationLinks({error:{code:'STOCK_RESERVED',message:'Stock is reserved for an order or POS work. Release the reservation or fulfill its source document first.'}}),{status:409});
  if(error instanceof Error&&error.message==='POS_STOCK_RESERVED')return Response.json(canonicalDestinationLinks({error:{code:'POS_STOCK_RESERVED',message:'Stock is reserved by POS checkout or kitchen work. Resolve the reservation before posting this movement or count.'}}),{status:409});
  if(error instanceof Error&&error.message.includes('OVERTIME_RECONCILIATION_REQUIRED'))return Response.json(canonicalDestinationLinks({error:{code:'OVERTIME_RECONCILIATION',message:'Approved overtime already supplies hours on this date. Reconcile manual hours before saving.'}}),{status:409});
  if (error instanceof ZodError) return Response.json(canonicalDestinationLinks({error:{code:"VALIDATION_FAILED",message:error.issues[0]?.message ?? "Invalid input."}}),{status:422});
  if (error instanceof ApiError) {
    const currentVersion=error.code==='PMS_VERSION_CONFLICT'?error.details?.currentVersion:undefined;
    return Response.json(canonicalDestinationLinks({ error: { code: error.code, message: error.message,
      ...(Number.isSafeInteger(currentVersion)&&Number(currentVersion)>0?{details:{currentVersion}}:{}) } }), { status: error.status });
  }
  const message = error instanceof Error ? error.message : "UNKNOWN";
  const mapped = message === "UNAUTHENTICATED"
    ? new ApiError(401, "UNAUTHENTICATED", "Sign in to continue.")
    : message === "FORBIDDEN"
      ? new ApiError(403, "FORBIDDEN", "This action is unavailable in your current role.")
      : ["HOLDING_NOT_FOUND", "WORKSPACE_NOT_FOUND", "INVALID_ORGANIZATION_LINEAGE"].includes(message)
        ? new ApiError(404, "NOT_FOUND", "Workspace not found.")
        : message === "COMPANY_REQUIRED"
          ? new ApiError(409, "COMPANY_REQUIRED", "Select an active Company to continue.")
        : new ApiError(500, "INTERNAL_ERROR", "The request could not be completed.");
  return Response.json(canonicalDestinationLinks({ error: { code: mapped.code, message: mapped.message } }), { status: mapped.status });
}
