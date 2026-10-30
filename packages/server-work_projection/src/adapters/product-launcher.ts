import {holdingRoutingEnabled, holdingFrontUiUrl} from '@erpx/feature-runtime/lib/holding-hosts';
import {expensePostingPrerequisitesProductEntry} from '@erpx/server-finance/public/expense-posting-prerequisites';
import {payrollPostingPrerequisitesProductEntry} from '@erpx/server-finance/public/payroll-posting-prerequisites';
import {peopleFinancialProductEntry} from "@erpx/server-people_payroll/public/financial-product-entry";
import {learningWorkerStartupProductEntry} from "@erpx/server-people_payroll/public/learning-worker-startup";
import {projectWorkerStartupProductEntry} from "@erpx/server-people_payroll/public/project-worker-startup";
import {servicePartsPostingSetupProductEntry} from "@erpx/server-commerce/public/service-parts-posting-setup";
import {assetsFinancialProductEntry} from "@erpx/server-finance/public/assets-financial-workflow";
import {posStartupProductEntry} from "@erpx/server-pos/public/startup";
import {manufacturingInitialPhysicalStockCountProductEntry} from "@erpx/server-inventory/public/manufacturing-initial-physical-stock-count";
import {manufacturingPostingSetupProductEntry} from "@erpx/server-manufacturing/public/posting-setup";
import {initialPhysicalStockCountProductEntry} from "@erpx/server-inventory/public/initial-physical-stock-count";
import "server-only";
import {developmentModuleAdmissionDeferred} from '@erpx/contracts/development-module-admission';
import { activeHoldingBindings, holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { firstAuthorizedWorkspace, requireAccountAccess, resolveWorkspaceScope, type AccountAccessContext } from "@erpx/server-identity_access/public/authorization";
import { readCompanyPurchaseFacts } from "@erpx/server-commercial/public/package-grants";
import { configuredProductOrigin } from "@erpx/feature-runtime/lib/product-hosts";
import { workspaceHref } from "@erpx/feature-work_projection/lib/workspace";
import { consoleOrigin } from "@erpx/feature-runtime/lib/erpx-hosts";
import { legacyConsoleRetired } from "@erpx/feature-runtime/lib/console-retirement";
import { productWorkspaceChoices } from "@erpx/server-organization/public/workspace-choices";
import { frontActiveCompanies } from "@erpx/server-organization/public/front-company-options";
import { assertProductionModuleEnabled } from "@erpx/server-pms/public/feature-availability";
import { ApiError } from "@erpx/server-runtime/public/http";
import { accountWorkModuleAccess } from "@erpx/server-identity_access/public/work-module-access";
import { createProductLauncher } from "@erpx/server-work_projection/application/product-launcher";

/** Migration recovery links use existing workspace authority, never new product grants. */
export async function existingWorkspaceLaunchChoices() {
  if (legacyConsoleRetired()) return [];
  const origin = consoleOrigin();
  if (!origin) return [];
  const bindings = await activeHoldingBindings();
  return (await Promise.all(bindings.map(async binding => {
    const scope = await firstAuthorizedWorkspace(binding.slug);
    return scope ? [{ holding: binding.displayName, href: new URL(workspaceHref(scope), origin).href }] : [];
  }))).flat();
}

/** Small server projection: no grants, permissions, or module catalogue enter Client props. */
export async function productLaunchChoices(productId?: import("@erpx/contracts/product-registry").ProductId) {
  // One render-local directory/session/assignment snapshot per Company, with
  // explicit node/lineage validation for each requested scope. Never global.
  const accounts = new Map<string, Promise<AccountAccessContext>>();
  return createProductLauncher({
  holdingAddresses:holdingRoutingEnabled(process.env),
  holdingFrontUrl:holdingFrontUiUrl,
  commercialAdmissionDeferred:id=>developmentModuleAdmissionDeferred(id,process.env),
  choices: productWorkspaceChoices, moduleAccess: accountWorkModuleAccess,
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
