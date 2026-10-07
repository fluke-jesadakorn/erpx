import {workspaceHref,type WorkspaceScope} from "@/lib/workspace";
import {safeReturnPath,type AccessProblem} from "@/lib/access-problem";
export type PageApiError=Error&{code:string;status:number};
export function createPageErrorService(ports:{headers():Promise<Pick<Headers,"get">>;requestedHoldingScope(slug:string):Promise<WorkspaceScope>;forbidden():never;notFound():never;redirect(path:string):never;unstableRethrow(error:unknown):void;isApiError(error:unknown):error is PageApiError}) {
async function currentPagePath(holdingSlug?: string) {
  const h = await ports.headers();
  const pagePath = h.get("x-erpx-page-path");
  const pageUrl = pagePath ? new URL(pagePath, "https://erpx.invalid") : null;
  const requested = safeReturnPath(pageUrl?.pathname === "/work-role" ? pageUrl.searchParams.get("from") : pagePath);
  if (requested) return requested;
  if (!holdingSlug) return "/platform/home";
  return safeReturnPath(workspaceHref(await ports.requestedHoldingScope(holdingSlug), h.get("x-erpx-destination") || "home")) ?? "/";
}
async function accessPage(reason: AccessProblem, holdingSlug?: string): Promise<never> {
  if (reason === "permission" || reason === "capability") ports.forbidden();
  ports.redirect(`/access?${new URLSearchParams({ reason, from: await currentPagePath(holdingSlug) })}`);
}
/** Unexpected failures reach an error boundary instead of becoming misleading 404s. */
async function pageError(error: unknown, holdingSlug?: string): Promise<never> {
  ports.unstableRethrow(error);
  const code = ports.isApiError(error) ? error.code : error instanceof Error ? error.message : "";
  if(code==="EMERGENCY_ACCESS_REQUIRED"||code==="FRESH_MFA_REQUIRED")ports.redirect("/security");
  if (["WORK_ROLE_REQUIRED","WORK_ROLE_UNAVAILABLE","WORK_CONTEXT_CHANGED","PLATFORM_CONTEXT_CHANGED"].includes(code)) ports.forbidden();
  if (code === "UNAUTHENTICATED" || ports.isApiError(error) && error.status === 401) {
    ports.redirect(`/login?${new URLSearchParams({ reason: "sign_in", next: await currentPagePath(holdingSlug) })}`);
  }
  if (["FORBIDDEN", "HOLDING_NOT_FOUND", "WORKSPACE_NOT_FOUND", "COMPANY_REQUIRED", "INVALID_ORGANIZATION_LINEAGE"].includes(code)) {
    ports.forbidden();
  }
  if (ports.isApiError(error)) {
    if (error.status === 404 || error.status === 410) ports.notFound();
    if (code === "BETA_CAPABILITY_UNAVAILABLE" || error.status === 403) ports.forbidden();
    if (error.status === 409) return accessPage("conflict", holdingSlug);
    if (error.status === 400 || error.status === 422) return accessPage("invalid", holdingSlug);
    if (error.status === 429) return accessPage("rate_limited", holdingSlug);
  }
  throw error;
}

return {currentPagePath,accessPage,pageError};
}
