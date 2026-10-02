import { headers } from "next/headers";
import { forbidden, notFound, redirect, unstable_rethrow } from "next/navigation";
import { requestedHoldingScope } from "./authorization";
import { workspaceHref } from "@/lib/workspace";
import { safeReturnPath, type AccessProblem } from "@/lib/access-problem";
import { ApiError } from "./http";

export async function currentPagePath(holdingSlug?: string) {
  const h = await headers();
  const pagePath = h.get("x-erpx-page-path");
  const pageUrl = pagePath ? new URL(pagePath, "https://erpx.invalid") : null;
  const requested = safeReturnPath(pageUrl?.pathname === "/work-role" ? pageUrl.searchParams.get("from") : pagePath);
  if (requested) return requested;
  if (!holdingSlug) return "/platform/home";
  return safeReturnPath(workspaceHref(await requestedHoldingScope(holdingSlug), h.get("x-erpx-destination") || "home")) ?? "/";
}
export async function accessPage(reason: AccessProblem, holdingSlug?: string): Promise<never> {
  if (reason === "permission" || reason === "capability") forbidden();
  redirect(`/access?${new URLSearchParams({ reason, from: await currentPagePath(holdingSlug) })}`);
}
/** Unexpected failures reach an error boundary instead of becoming misleading 404s. */
export async function pageError(error: unknown, holdingSlug?: string): Promise<never> {
  unstable_rethrow(error);
  const code = error instanceof ApiError ? error.code : error instanceof Error ? error.message : "";
  if(code==="EMERGENCY_ACCESS_REQUIRED"||code==="FRESH_MFA_REQUIRED")redirect("/security");
  if (["WORK_ROLE_REQUIRED","WORK_ROLE_UNAVAILABLE","WORK_CONTEXT_CHANGED","PLATFORM_CONTEXT_CHANGED"].includes(code)) forbidden();
  if (code === "UNAUTHENTICATED" || error instanceof ApiError && error.status === 401) {
    redirect(`/login?${new URLSearchParams({ reason: "sign_in", next: await currentPagePath(holdingSlug) })}`);
  }
  if (["FORBIDDEN", "HOLDING_NOT_FOUND", "WORKSPACE_NOT_FOUND", "COMPANY_REQUIRED", "INVALID_ORGANIZATION_LINEAGE"].includes(code)) {
    forbidden();
  }
  if (error instanceof ApiError) {
    if (error.status === 404 || error.status === 410) notFound();
    if (code === "BETA_CAPABILITY_UNAVAILABLE" || error.status === 403) forbidden();
    if (error.status === 409) return accessPage("conflict", holdingSlug);
    if (error.status === 400 || error.status === 422) return accessPage("invalid", holdingSlug);
    if (error.status === 429) return accessPage("rate_limited", holdingSlug);
  }
  throw error;
}
