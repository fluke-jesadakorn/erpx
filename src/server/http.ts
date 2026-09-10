import { ZodError } from "zod";
export class ApiError extends Error {
  constructor(
    public status: number,
    public code: string,
    message: string,
  ) {
    super(message);
  }
}

export function errorResponse(error: unknown) {
  if (error instanceof ZodError) return Response.json({error:{code:"VALIDATION_FAILED",message:error.issues[0]?.message ?? "Invalid input."}},{status:422});
  if (error instanceof ApiError) {
    return Response.json({ error: { code: error.code, message: error.message } }, { status: error.status });
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
  return Response.json({ error: { code: mapped.code, message: mapped.message } }, { status: mapped.status });
}
