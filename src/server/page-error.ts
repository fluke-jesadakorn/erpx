import { headers } from "next/headers";
import { notFound, redirect } from "next/navigation";
import { requestedHoldingScope } from "./authorization";
import { workspaceHref } from "@/lib/workspace";
import { safeReturnPath, type AccessProblem } from "@/lib/access-problem";
import { ApiError } from "./http";

export async function currentPagePath(holdingSlug?: string) {
  if (!holdingSlug) return "/platform/home";
  const h = await headers();
  return safeReturnPath(workspaceHref(await requestedHoldingScope(holdingSlug), h.get("x-erpx-destination") || "home")) ?? "/";
}
export async function accessPage(reason: AccessProblem, holdingSlug?: string): Promise<never> {
  redirect(`/access?${new URLSearchParams({ reason, from: await currentPagePath(holdingSlug) })}`);
}
/** Unexpected failures reach an error boundary instead of becoming misleading 404s. */
export async function pageError(error: unknown, holdingSlug?: string): Promise<never> {
  const code = error instanceof ApiError ? error.code : error instanceof Error ? error.message : "";
  if(code==="EMERGENCY_ACCESS_REQUIRED"||code==="FRESH_MFA_REQUIRED")redirect("/security");
  if (["WORK_ROLE_REQUIRED","WORK_ROLE_UNAVAILABLE","WORK_CONTEXT_CHANGED"].includes(code)) redirect(`/work-role?${new URLSearchParams({from:await currentPagePath(holdingSlug)})}`);
  if (code === "UNAUTHENTICATED" || error instanceof ApiError && error.status === 401) {
    redirect(`/login?${new URLSearchParams({ reason: "sign_in", next: await currentPagePath(holdingSlug) })}`);
  }
  if (["FORBIDDEN", "HOLDING_NOT_FOUND", "WORKSPACE_NOT_FOUND", "COMPANY_REQUIRED", "INVALID_ORGANIZATION_LINEAGE"].includes(code)) {
    redirect(`/access?${new URLSearchParams({ from: await currentPagePath(holdingSlug) })}`);
  }
  if (error instanceof ApiError) {
    if (error.status === 404 || error.status === 410) notFound();
    if (code === "BETA_CAPABILITY_UNAVAILABLE") return accessPage("capability", holdingSlug);
    if (error.status === 403) return accessPage("permission", holdingSlug);
    if (error.status === 409) return accessPage("conflict", holdingSlug);
    if (error.status === 400 || error.status === 422) return accessPage("invalid", holdingSlug);
    if (error.status === 429) return accessPage("rate_limited", holdingSlug);
  }
  throw error;
}
