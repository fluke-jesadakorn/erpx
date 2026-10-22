import {expensePostingPrerequisitesProductEntry} from '@erpx/server-finance/public/expense-posting-prerequisites';
import {payrollPostingPrerequisitesProductEntry} from '@erpx/server-finance/public/payroll-posting-prerequisites';
import {peopleFinancialProductEntry} from "@erpx/server-people_payroll/public/financial-product-entry";
import {learningWorkerStartupProductEntry} from "@erpx/server-people_payroll/public/learning-worker-startup";
import {projectWorkerStartupProductEntry} from "@erpx/server-people_payroll/public/project-worker-startup";
import {serviceInitialPhysicalStockCountProductEntry} from "@erpx/server-inventory/public/service-initial-physical-stock-count";
import {servicePartsPostingSetupProductEntry} from "@erpx/server-commerce/public/service-parts-posting-setup";
import {posInitialPhysicalStockCountProductEntry} from "@erpx/server-inventory/public/pos-initial-physical-stock-count";
import {assetsFinancialProductEntry} from "@erpx/server-finance/public/assets-financial-workflow";
import {posStartupProductEntry} from "@erpx/server-pos/public/startup";
import {manufacturingInitialPhysicalStockCountProductEntry} from "@erpx/server-inventory/public/manufacturing-initial-physical-stock-count";
import {manufacturingPostingSetupProductEntry} from "@erpx/server-manufacturing/public/posting-setup";
import {initialPhysicalStockCountProductEntry} from "@erpx/server-inventory/public/initial-physical-stock-count";
import {salesCustomerReceiptProductEntry} from "@erpx/server-finance/public/sales-receipt-sources";
import {commercePostingSetupProductEntry} from "@erpx/server-commerce/public/posting-setup";
import {salesPostingSetupProductEntry} from "@erpx/server-finance/public/sales-posting-setup";
import {workforceProductSelfServiceEntry} from '@erpx/server-people_payroll/public/self-service-product-entry';
import {socialPostProductSourceEntry} from '@erpx/server-collaboration/public/product-entry';
import "server-only";
import {financeProductLegalSetupEntry} from "@erpx/server-finance/public/legal-setup";
import {financeProductDocumentEntry} from "@erpx/server-finance/public/document-purchases";
import {qualityProductSourceEntry} from "@erpx/server-quality/public/product-entry";
import {projectTaskProductSourceEntry} from "@erpx/server-projects/public/product-entry";
import {talentHireProductSourceEntry,talentCandidatePrivacyProductSourceEntry} from "@erpx/server-talent/public/product-entry";
import { NextResponse } from "next/server";
import { productHostForHost, configuredProductOrigin } from "@erpx/feature-runtime/lib/product-hosts";
import { ApiError, errorResponse } from "@erpx/server-runtime/public/http";
import { requireAccountAccess } from "@erpx/server-identity_access/adapters/authorization";
import { productWorkspaceTarget } from "@erpx/server-identity_access/domain/product-workspace";
import { createProductWorkspace, type ProductWorkspaceInput } from "@erpx/server-identity_access/application/product-workspace";
import { accountWorkModuleAccess } from "@erpx/server-identity_access/adapters/work-module-access";
import { readCompanyPurchaseFacts } from "@erpx/server-commercial/public/package-grants";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { initializeWorkSelection, selectWorkRole } from "@erpx/server-work_projection/public/work-role-store";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import { executiveVisible as isExecutiveVisible } from "@erpx/server-runtime/public/executive/access";
import { loadIamAuthority } from "@erpx/server-identity_access/adapters/iam-store";
import { betaAccess } from "@erpx/server-identity_access/adapters/beta-access";
import { effectiveWorkModules } from "@erpx/server-identity_access/domain/work-modules";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { productCompany } from "@erpx/server-organization/public/product-company";
import { z } from "zod";

const responseHeaders = { "Cache-Control": "private, no-store", "Referrer-Policy": "no-referrer" };
const formSchema = z.object({ companyId: z.string().min(1).max(128), next: z.string().min(1).max(2000),
  assignmentId: z.string().min(1).max(128), assignmentVersion: z.coerce.number().int().positive(), revision: z.coerce.number().int().nonnegative() }).strict();
function service() {
  return createProductWorkspace({
    access: input => requireAccountAccess(input.scope, { companyId: input.companyId }),
    modules: accountWorkModuleAccess,
    purchaseFacts: async access => readCompanyPurchaseFacts((await holdingResources(access.scope.holdingId)).write, access.scope.companyId),
    ready: async (access, moduleId) => {
      try { await assertProductionModuleEnabled((await holdingResources(access.scope.holdingId)).write, access.scope.companyId, moduleId); return true; }
      catch (error) { if (error instanceof ApiError && error.code === "FEATURE_COMING_SOON") return false; throw error; }
    },
    executiveVisible: async (access, assignment) => {
      const { write } = await holdingResources(access.scope.holdingId);
      const [iam, beta] = await Promise.all([
        loadIamAuthority(write, access.scope.companyId, access.userId, assignment.role, access.sessionId),
        betaAccess(write, access.scope.companyId, new Headers()),
      ]);
      const actor: WorkContext = {
        iam, userId: access.userId, name: access.name, email: access.email, assignmentId: assignment.id,
        role: assignment.role, companyId: access.scope.companyId,
        branchId: access.scope.kind === "holding" ? null : access.scope.branchId,
        departmentId: access.scope.kind === "department" ? access.scope.departmentId : null,
        holdingId: access.scope.holdingId, holdingSlug: access.scope.holdingSlug, holdingName: access.scope.holdingName,
        scope: access.scope, assignmentScope: assignment.scopeKind, inherited: assignment.scopeKind !== access.scope.kind,
        modules: effectiveWorkModules(assignment.role, assignment.modules, iam.permissions, beta),
        assignments: [assignment], availableAssignments: access.assignments, beta, workspaces: [],
      };
      return isExecutiveVisible(write, actor);
    },
    selection: async (access, applicable) => initializeWorkSelection((await holdingResources(access.scope.holdingId)).write,
      { sessionId: access.sessionId, userId: access.userId, companyId: access.scope.companyId, expiresAt: access.expiresAt }, access.assignments, applicable),
    select: async (access, option, revision) => selectWorkRole((await holdingResources(access.scope.holdingId)).write,
      { sessionId: access.sessionId, userId: access.userId, companyId: access.scope.companyId, expiresAt: access.expiresAt,
        assignmentId: option.assignmentId, assignmentVersion: option.assignmentVersion, revision }),
    commercePostingSetupEntry:async(access,assignment)=>commercePostingSetupProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId}),
    salesPostingSetupEntry:async(access,assignment)=>salesPostingSetupProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId}),
    expensePostingPrerequisitesEntry:async(access,assignment)=>expensePostingPrerequisitesProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId}),
    payrollPostingPrerequisitesEntry:async(access,assignment)=>payrollPostingPrerequisitesProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId}),
    peopleFinancialEntry:async(access,assignment,target)=>peopleFinancialProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},target),
    assetsFinancialEntry:async(access,assignment,target)=>assetsFinancialProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},target),
    posInitialPhysicalStockCountEntry:async(access,assignment,target)=>posInitialPhysicalStockCountProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},target),
    learningWorkerStartupEntry:async(access,assignment,target)=>learningWorkerStartupProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},target),
    projectWorkerStartupEntry:async(access,assignment,target)=>projectWorkerStartupProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},target),
    servicePartsSetupEntry:async(access,assignment)=>servicePartsPostingSetupProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId}),
    serviceInitialPhysicalStockCountEntry:async(access,assignment,target)=>serviceInitialPhysicalStockCountProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},target),
    posStartupEntry:async(access,assignment)=>posStartupProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId}),
    manufacturingPostingSetupEntry:async(access,assignment)=>manufacturingPostingSetupProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId}),
    manufacturingInitialPhysicalStockCountEntry:async(access,assignment,target)=>manufacturingInitialPhysicalStockCountProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},target),
    initialPhysicalStockCountEntry:async(access,assignment,target)=>initialPhysicalStockCountProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},target),
    salesCustomerReceiptEntry:async(access,assignment,target)=>salesCustomerReceiptProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},target.id),
    financeLegalSetupEntry:async(access,assignment)=>financeProductLegalSetupEntry({holdingId:access.scope.holdingId,companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role}),
    financeDocumentEntry:async(access,assignment,target)=>financeProductDocumentEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role},target),
    qualitySourceEntry:async(access,assignment,target)=>qualityProductSourceEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role},target),
    workforceSelfServiceEntry:async(access,assignment,target)=>workforceProductSelfServiceEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},target),
    socialPostSourceEntry:async(access,assignment,target)=>socialPostProductSourceEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role},target),
    projectTaskSourceEntry:async(access,assignment,target)=>projectTaskProductSourceEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role},target),
    talentHireSourceEntry:async(access,assignment,target)=>talentHireProductSourceEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role},target.id),
    talentCandidatePrivacySourceEntry:async(access,assignment,target)=>talentCandidatePrivacyProductSourceEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role},target.id),
    now: Date.now, error: (status, code, message) => new ApiError(status, code, message),
  });
}
export function productWorkRolePath(input: ProductWorkspaceInput, notice?: string) {
  const query = new URLSearchParams({ companyId: input.companyId, next: input.target, ...(notice ? { notice } : {}) });
  return "/choose-work-role?" + query;
}
export function productLaunchPath(input: ProductWorkspaceInput) {
  return "/launch?" + new URLSearchParams({ companyId: input.companyId, next: input.target });
}
export async function productWorkRoleChoices(audience: Parameters<typeof productWorkspaceTarget>[0], input: ProductWorkspaceInput) {
  const state = await service().read(audience, input);
  const company = await productCompany((await holdingResources(state.access.scope.holdingId)).write, state.access.scope.companyId);
  if (!company) throw new ApiError(404, "COMPANY_NOT_FOUND", "Select an active Company.");
  // No account/authority snapshots are exposed to the page's Client boundary.
  return { company, options: state.options.map(option => ({ assignmentId: option.assignmentId, assignmentVersion: option.assignmentVersion, label: option.label, purchasePurpose: option.purchasePurpose })),
    revision: state.selection?.revision ?? 0, target: state.target };
}

/** The owner page must repeat the entry decision on a direct URL. Reading the
 * decision never substitutes another available role for the current role. */
export async function productPageAccess(audience: Parameters<typeof productWorkspaceTarget>[0], companyId: string, path: string) {
  const input = productWorkspaceTarget(audience, companyId, path);
  if (!input) throw new ApiError(404, "PRODUCT_PAGE_NOT_FOUND", "This page does not belong to the selected product.");
  const state = await service().read(audience, input);
  return { target: state.target, chooseRole: productWorkRolePath(input), sourceRecord:state.sourceRecord };
}

function allowsProductWorkspaceFormOrigin(request: Request, origin: string) {
  if (request.headers.get("origin") === origin) return true;
  const url = new URL(request.url);
  return request.method === "POST" && url.pathname === "/launch" && request.headers.get("origin") === "null"
    && request.headers.get("sec-fetch-site") === "same-origin"
    && request.headers.get("sec-fetch-mode") === "navigate"
    && request.headers.get("sec-fetch-dest") === "document";
}

export async function productWorkspaceRequest(request: Request) {
  const audience = productHostForHost(request.headers.get("host"));
  const query = new URL(request.url).searchParams;
  let input = audience && productWorkspaceTarget(audience, query.get("companyId"), query.get("next"));
  try {
    if (!audience) return new Response("Invalid workspace", { status: 400, headers: responseHeaders });
    if (request.method === "POST") {
      const origin = configuredProductOrigin(audience);
      if (!origin || !allowsProductWorkspaceFormOrigin(request, origin)) throw new ApiError(403, "ORIGIN_FORBIDDEN", "Choose a work role from this product.");
      if (!request.headers.get("content-type")?.startsWith("application/x-www-form-urlencoded")) throw new ApiError(415, "FORM_REQUIRED", "Submit the work role form.");
      const body = await request.text();
      if (body.length > 8192) throw new ApiError(413, "REQUEST_TOO_LARGE", "The work role request is too large.");
      const values = new URLSearchParams(body);
      if ([...new Set(values.keys())].some(key => values.getAll(key).length !== 1)) throw new ApiError(422, "INVALID_FORM", "Submit one work role choice.");
      const form = formSchema.parse(Object.fromEntries(values));
      input = productWorkspaceTarget(audience, form.companyId, form.next);
      if (!input) throw new ApiError(400, "INVALID_PRODUCT_WORKSPACE", "Select a valid product workspace.");
      try { await service().choose(audience, input, form.assignmentId, form.revision, form.assignmentVersion); }
      catch (error) {
        if (error instanceof ApiError && [403, 409].includes(error.status)) return NextResponse.redirect(
          new URL(productWorkRolePath(input, "access-changed"), origin), { status: 303, headers: responseHeaders });
        throw error;
      }
      return NextResponse.redirect(new URL(productLaunchPath(input), origin), { status: 303, headers: responseHeaders });
    }
    if (request.method !== "GET" || !input) return new Response("Invalid workspace", { status: 400, headers: responseHeaders });
    const state = await service().read(audience, input), access = state.access;
    const target = state.target ?? productWorkRolePath(input);
    const response = NextResponse.redirect(new URL(target, configuredProductOrigin(audience)!), { status: 303, headers: responseHeaders });
    response.cookies.set(`erpx_company_${access.scope.holdingId}`, access.scope.companyId, {
      secure: true, httpOnly: true, sameSite: "lax", path: "/", maxAge: 60 * 60 * 24 * 365,
    });
    return response;
  } catch (error) {
    const response = errorResponse(error);
    for (const [key, value] of Object.entries(responseHeaders)) response.headers.set(key, value);
    return response;
  }
}
