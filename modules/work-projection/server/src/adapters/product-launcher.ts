import {holdingFrontUiUrl} from '@erpx/platform-runtime-ui/lib/holding-hosts';
import {holdingRoutingEnabled} from '@erpx/platform-contracts/holding-hosts';
import {expensePostingPrerequisitesProductEntry} from '@erpx/finance-server/public/expense-posting-prerequisites';
import {payrollPostingPrerequisitesProductEntry} from '@erpx/finance-server/public/payroll-posting-prerequisites';
import {peopleFinancialProductEntry} from "@erpx/people-payroll-server/public/financial-product-entry";
import {learningWorkerStartupProductEntry} from "@erpx/people-payroll-server/public/learning-worker-startup";
import {projectWorkerStartupProductEntry} from "@erpx/people-payroll-server/public/project-worker-startup";
import {servicePartsPostingSetupProductEntry} from "@erpx/commerce-server/public/service-parts-posting-setup";
import {assetsFinancialProductEntry} from "@erpx/finance-server/public/assets-financial-workflow";
import {posStartupProductEntry} from "@erpx/pos-server/public/startup";
import {manufacturingInitialPhysicalStockCountProductEntry} from "@erpx/inventory-server/public/manufacturing-initial-physical-stock-count";
import {manufacturingPostingSetupProductEntry} from "@erpx/manufacturing-server/public/posting-setup";
import {initialPhysicalStockCountProductEntry} from "@erpx/inventory-server/public/initial-physical-stock-count";
import "server-only";
import {developmentModuleAdmissionDeferred} from '@erpx/platform-contracts/development-module-admission';
import { activeHoldingBindings, holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { firstAuthorizedWorkspace, requireAccountAccess, resolveWorkspaceScope, type AccountAccessContext } from "@erpx/identity-access-server/public/authorization";
import { readCompanyPurchaseFacts } from "@erpx/commercial-server/public/package-grants";
import { configuredProductOrigin } from "@erpx/platform-runtime-ui/lib/product-hosts";
import { workspaceHref } from "@erpx/work-projection-ui/lib/workspace";
import { consoleOrigin } from "@erpx/platform-runtime-ui/lib/erpx-hosts";
import { legacyConsoleRetired } from "@erpx/platform-runtime-ui/lib/console-retirement";
import { productWorkspaceChoices } from "@erpx/organization-server/public/workspace-choices";
import { frontActiveCompanies } from "@erpx/organization-server/public/front-company-options";
import { assertProductionModuleEnabled } from "@erpx/pms-server/public/feature-availability";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { accountWorkModuleAccess } from "@erpx/identity-access-server/public/work-module-access";
import { createProductLauncher } from "@erpx/work-projection-server/application/product-launcher";

/** Migration recovery links use existing workspace authority, never new product grants. */
export async function existingWorkspaceLaunchChoices() {
  if (legacyConsoleRetired()) return [];
  const origin = consoleOrigin();
  if (!origin) return [];
  const bindings = await activeHoldingBindings();
  return (await Promise.all(bindings.map(async binding => {
    try {
      const scope = await firstAuthorizedWorkspace(binding.slug);
      return scope ? [{ holding: binding.displayName, href: new URL(workspaceHref(scope), origin).href }] : [];
    } catch (error) {
      // A registered binding may belong to another environment or no longer be
      // present in this native workspace configuration. Recovery links are
      // best-effort; an unavailable binding must not blank the product entry.
      if (error instanceof ApiError && [401, 403, 404].includes(error.status)) return [];
      throw error;
    }
  }))).flat();
}

/** Small server projection: no grants, permissions, or module catalogue enter Client props. */
export async function productLaunchChoices(productId?: import("@erpx/platform-contracts/product-registry").ProductId, holdingSlug?: string) {
  // One render-local directory/session/assignment snapshot per Company, with
  // explicit node/lineage validation for each requested scope. Never global.
  const accounts = new Map<string, Promise<AccountAccessContext>>();
  return createProductLauncher({
  holdingAddresses:holdingRoutingEnabled(process.env),
  holdingFrontUrl:holdingFrontUiUrl,
  commercialAdmissionDeferred:id=>developmentModuleAdmissionDeferred(id,process.env),
  choices: productWorkspaceChoices, moduleAccess: accountWorkModuleAccess,
  holdingSlug,
  // Native release checks depend only on Holding, Company and module, so many
  // workspace candidates in one render share the same readiness probe.
  readyKey: (access, moduleId) => `${access.scope.holdingId}:${access.scope.companyId}:${moduleId}`,
  access: async (scope, options) => {
    const key = `${scope.holdingSlug}:${options.companyId}`;
    let pending = accounts.get(key);
    if (!pending) {
      pending = requireAccountAccess({ kind: "holding", holdingSlug: scope.holdingSlug }, options);
      accounts.set(key, pending);
    }
    const account = await pending;
    return scope.kind === "holding" ? account : { ...account, scope: await resolveWorkspaceScope(scope, options) };
  },
  purchaseFacts: async access => readCompanyPurchaseFacts((await holdingResources(access.scope.holdingId)).write, access.scope.companyId),
  ready: async (access, moduleId) => {
    try { await assertProductionModuleEnabled((await holdingResources(access.scope.holdingId)).write, access.scope.companyId, moduleId); return true; }
    catch (error) { if (error instanceof ApiError && error.code === "FEATURE_COMING_SOON") return false; throw error; }
  },
  expensePostingPrerequisitesEntry:async(access,assignment)=>expensePostingPrerequisitesProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId}),
  payrollPostingPrerequisitesEntry:async(access,assignment)=>payrollPostingPrerequisitesProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId}),
  peopleFinancialEntry:async(access,assignment,target)=>peopleFinancialProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},target),
  assetsFinancialEntry:async(access,assignment)=>assetsFinancialProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId}),
  learningWorkerStartupEntry:async(access,assignment)=>learningWorkerStartupProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},{}),
  projectWorkerStartupEntry:async(access,assignment)=>projectWorkerStartupProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},{}),
  servicePartsSetupEntry:async(access,assignment)=>servicePartsPostingSetupProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId}),
  posStartupEntry:async(access,assignment)=>posStartupProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId}),
  manufacturingPostingSetupEntry:async(access,assignment)=>manufacturingPostingSetupProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId}),
  manufacturingInitialPhysicalStockCountEntry:async(access,assignment)=>manufacturingInitialPhysicalStockCountProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},{}),
  initialPhysicalStockCountEntry:async(access,assignment)=>initialPhysicalStockCountProductEntry((await holdingResources(access.scope.holdingId)).write,{companyId:access.scope.companyId,branchId:"branchId" in access.scope?access.scope.branchId:null,departmentId:access.scope.kind==="department"?access.scope.departmentId:null,userId:access.userId,assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,sessionId:access.sessionId},{}),
  companySlug: async access => (await frontActiveCompanies((await holdingResources(access.scope.holdingId)).write, [access.scope.companyId]))[0]?.slug ?? null,
  origin: (id,slug)=>configuredProductOrigin(id,undefined,slug),
  denied: error => error instanceof ApiError && [401, 403, 404].includes(error.status) || error instanceof Error && ["FORBIDDEN", "WORKSPACE_NOT_FOUND", "COMPANY_REQUIRED"].includes(error.message),
  now: Date.now,
  })(productId);
}
