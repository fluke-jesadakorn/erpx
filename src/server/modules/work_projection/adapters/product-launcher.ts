import {expensePostingPrerequisitesProductEntry} from '../../finance/public/expense-posting-prerequisites';
import {payrollPostingPrerequisitesProductEntry} from '../../finance/public/payroll-posting-prerequisites';
import {peopleFinancialProductEntry} from "../../people_payroll/public/financial-product-entry";
import {learningWorkerStartupProductEntry} from "../../people_payroll/public/learning-worker-startup";
import {projectWorkerStartupProductEntry} from "../../people_payroll/public/project-worker-startup";
import {servicePartsPostingSetupProductEntry} from "../../commerce/public/service-parts-posting-setup";
import {assetsFinancialProductEntry} from "../../finance/public/assets-financial-workflow";
import {posStartupProductEntry} from "../../pos/public/startup";
import {manufacturingInitialPhysicalStockCountProductEntry} from "../../inventory/public/manufacturing-initial-physical-stock-count";
import {manufacturingPostingSetupProductEntry} from "../../manufacturing/public/posting-setup";
import {initialPhysicalStockCountProductEntry} from "../../inventory/public/initial-physical-stock-count";
import "server-only";
import { activeHoldingBindings, holdingResources } from "@/server/modules/runtime/public/runtime";
import { firstAuthorizedWorkspace, requireAccountAccess, resolveWorkspaceScope, type AccountAccessContext } from "@/server/modules/identity_access/public/authorization";
import { readCompanyPurchaseFacts } from "@/server/modules/commercial/public/package-grants";
import { configuredProductOrigin } from "@/lib/runtime/product-hosts";
import { workspaceHref } from "@/lib/work_projection/workspace";
import { consoleOrigin } from "@/lib/runtime/erpx-hosts";
import { legacyConsoleRetired } from "@/lib/runtime/console-retirement";
import { productWorkspaceChoices } from "@/server/modules/organization/public/workspace-choices";
import { frontActiveCompanies } from "@/server/modules/organization/public/front-company-options";
import { assertProductionModuleEnabled } from "@/server/modules/pms/public/hospitality-rollout";
import { ApiError } from "@/server/modules/runtime/public/http";
import { accountWorkModuleAccess } from "../../identity_access/public/work-module-access";
import { createProductLauncher } from "../application/product-launcher";

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
export async function productLaunchChoices() {
  // One render-local directory/session/assignment snapshot per Company, with
  // explicit node/lineage validation for each requested scope. Never global.
  const accounts = new Map<string, Promise<AccountAccessContext>>();
  return createProductLauncher({
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
  origin: configuredProductOrigin,
  denied: error => error instanceof ApiError && [401, 403, 404].includes(error.status) || error instanceof Error && ["FORBIDDEN", "WORKSPACE_NOT_FOUND", "COMPANY_REQUIRED"].includes(error.message),
  now: Date.now,
  })();
}
