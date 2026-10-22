import {expensePostingPrerequisitesLanding,expensePostingPrerequisitesPresentationAllowed} from '@erpx/server-finance/public/rules/expense-posting-prerequisites-entry';
import {payrollPostingPrerequisitesLanding,payrollPostingPrerequisitesPresentationAllowed} from '@erpx/server-finance/public/rules/payroll-posting-prerequisites-entry';
import {expenseFinancialLanding,payrollFinancialLanding,peopleFinancialPresentationAllowed,type PeopleFinancialEntry} from '@erpx/server-people_payroll/public/rules/financial-product-entry';
import {learningWorkerStartupLanding,learningWorkerPresentationAllowed} from "@erpx/server-people_payroll/public/rules/learning-worker-startup-entry";
import {projectWorkerStartupLanding,projectWorkerPresentationAllowed} from "@erpx/server-people_payroll/public/rules/project-worker-startup-entry";
import {servicePartsSetupLanding,servicePartsPresentationAllowed} from "@erpx/server-field_service/public/rules/parts-entry";
import {assetsFinancialLanding,assetsFinancialPresentationAllowed} from "@erpx/server-finance/public/rules/assets-financial-entry";
import {posStartupLanding,posStartupPresentationAllowed} from "@erpx/server-pos/public/rules/startup-entry";
import {manufacturingInitialPhysicalStockCountLanding} from "@erpx/server-inventory/public/rules/manufacturing-initial-physical-stock-count";
import {manufacturingPostingSetupLanding} from "@erpx/server-manufacturing/public/rules/posting-setup-entry";
import {initialPhysicalStockCountLanding} from "@erpx/server-inventory/public/rules/initial-physical-stock-count";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import type { WorkspaceChoice } from "@erpx/feature-work_projection/lib/workspace-choice";
import { parseHoldingWorkspace, workspaceBase, type HoldingWorkspaceScope } from "@erpx/feature-work_projection/lib/workspace";
import { products, type ProductId } from "@erpx/server-commercial/public/rules/product-registry";
import { visiblePurchasedProducts, purchasedCapabilities, purchasedCapabilityPurpose, type CompanyPurchaseFacts, type CapabilityPurpose } from "@erpx/server-commercial/public/rules/packages";
import { productEntryCapability } from "@erpx/server-commercial/public/rules/product-entry-capability";
import { assignmentApplies } from "@erpx/server-identity_access/public/rules/workspace-access";
import type { AccountAccessContext } from "@erpx/server-identity_access/public/authorization";
import type { ProductModuleAccess } from "@erpx/server-identity_access/public/rules/product-module-access";
import { productOfficeLanding } from "@erpx/server-work_projection/domain/product-landings";

export type ProductWorkspaceCandidate = { holding: string; company: string; companyId: string; workspace: string; scope: HoldingWorkspaceScope };
export function productWorkspaceCandidates(choices: readonly WorkspaceChoice[]): ProductWorkspaceCandidate[] {
  return choices.flatMap(holding => holding.kind === "platform" ? [] : holding.companies.flatMap(company => {
    const candidates = [
      { destination: company.destination, label: company.label },
      ...company.branches.flatMap(branch => [{ destination: branch.destination, label: branch.label }, ...branch.departments.map(department => ({ destination: department.destination, label: `${branch.label} / ${department.label}` }))]),
      ...company.departments.map(department => ({ destination: department.destination, label: department.label })),
    ];
    return candidates.flatMap(candidate => {
      if (!candidate.destination) return [];
      const parts = new URL(candidate.destination, "https://erpx.invalid").pathname.split("/").filter(Boolean).map(decodeURIComponent);
      const parsed = parts[0] === "holdings" ? parseHoldingWorkspace(parts[1], parts.slice(2)) : null;
      return parsed ? [{ holding: holding.label, company: company.label, companyId: company.id, workspace: candidate.label, scope: parsed.scope }] : [];
    });
  }));
}

export function createProductLauncher(ports: {
  choices(): Promise<WorkspaceChoice[]>;
  access(scope: HoldingWorkspaceScope, options: { companyId: string }): Promise<AccountAccessContext>;
  purchaseFacts(access: AccountAccessContext): Promise<CompanyPurchaseFacts>;
  moduleAccess(access: AccountAccessContext): Promise<ProductModuleAccess>;
  ready(access: AccountAccessContext, moduleId: string): Promise<boolean>;
  expensePostingPrerequisitesEntry?(access:AccountAccessContext,assignment:AccountAccessContext["assignments"][number]):Promise<{revision:string;purchasePurpose:"history"}|null>;
  payrollPostingPrerequisitesEntry?(access:AccountAccessContext,assignment:AccountAccessContext["assignments"][number]):Promise<{revision:string;purchasePurpose:"history"}|null>;
  peopleFinancialEntry?(access:AccountAccessContext,assignment:AccountAccessContext["assignments"][number],target:PeopleFinancialEntry):Promise<{revision:string;purchasePurpose:"history"}|null>;
  assetsFinancialEntry?(access:AccountAccessContext,assignment:AccountAccessContext["assignments"][number]):Promise<{revision:string;purchasePurpose:"history"}|null>;
  learningWorkerStartupEntry?(access:AccountAccessContext,assignment:AccountAccessContext["assignments"][number]):Promise<{revision:string;purchasePurpose:"history"}|null>;
  projectWorkerStartupEntry?(access:AccountAccessContext,assignment:AccountAccessContext["assignments"][number]):Promise<{revision:string;purchasePurpose:"history"}|null>;
  servicePartsSetupEntry?(access:AccountAccessContext,assignment:AccountAccessContext["assignments"][number]):Promise<{revision:string;purchasePurpose:"history"}|null>;
  posStartupEntry?(access:AccountAccessContext,assignment:AccountAccessContext["assignments"][number]):Promise<{revision:string;purchasePurpose:"history"}|null>;
  manufacturingPostingSetupEntry?(access:AccountAccessContext,assignment:AccountAccessContext["assignments"][number]):Promise<{revision:string;purchasePurpose:"history"}|null>;
  manufacturingInitialPhysicalStockCountEntry?(access:AccountAccessContext,assignment:AccountAccessContext["assignments"][number]):Promise<{capability:string;purchasePurpose:"history"}|null>;
  initialPhysicalStockCountEntry?(access:AccountAccessContext,assignment:AccountAccessContext["assignments"][number]):Promise<{capability:string;purchasePurpose:"history"}|null>;
  companySlug(access: AccountAccessContext): Promise<string | null>;
  origin(id: ProductId): string | null;
  denied(error: unknown): boolean;
  now(): number;
}) {
  return async (productId?: ProductId) => {
    const purchaseReads = new Map<string, Promise<CompanyPurchaseFacts>>();
    const moduleReads = new Map<string, ReturnType<typeof ports.moduleAccess>>();
    const result: (ProductWorkspaceCandidate & { productId: ProductId; href: string; purchasePurpose: CapabilityPurpose; sourceRecord?:true })[] = [];
    for (const candidate of productWorkspaceCandidates(await ports.choices())) {
      let access: AccountAccessContext;
      try { access = await ports.access(candidate.scope, { companyId: candidate.companyId }); }
      catch (error) { if (ports.denied(error)) continue; throw error; }
      const key = `${access.scope.holdingId}:${access.scope.companyId}`;
      let pending = purchaseReads.get(key);
      if (!pending) { pending = ports.purchaseFacts(access); purchaseReads.set(key, pending); }
      const facts = await pending;
      const purchased = visiblePurchasedProducts(facts.grants, candidate.companyId, ports.now(), "history", facts.history);
      if (!purchased.length) continue;
      let moduleRead = moduleReads.get(key);
      if (!moduleRead) { moduleRead = ports.moduleAccess(access); moduleReads.set(key, moduleRead); }
      const authority = await moduleRead;
      const applicable = access.assignments.filter(assignment => assignmentApplies(assignment, access.scope) && authority.modulesByAssignment.has(assignment.id));
      const readyReads = new Map<string, Promise<boolean>>();
      const readyModule = (id: string) => {
        let read = readyReads.get(id);
        if (!read) { read = ports.ready(access, id); readyReads.set(id, read); }
        return read;
      };
      for (const definition of products) {
        if (productId && definition.id !== productId) continue;
        const origin = ports.origin(definition.id);
        const expenseIncluded=definition.id==="finance"&&purchasedCapabilityPurpose(facts,candidate.companyId,"people.expenses",ports.now())!==null;
        if (!origin || !purchased.includes(definition.id)&&!expenseIncluded) continue;
        let landing: string | null = null;
        let purchasePurpose: CapabilityPurpose | null = null;
        let sourceRecord=false;
        for (const assignment of applicable) {
          if (!authority.permissionsByAssignment.has(assignment.id)) continue;
          const modules = authority.presentationModulesByAssignment.get(assignment.id) ?? [];
          const ready = (await Promise.all([...new Set([...modules, ...(definition.id === "work" ? ["operations"] : [])])]
            .map(async id => await readyModule(id) ? id : null))).filter((id): id is string => id !== null);
          let roleLanding: string | null = null;
          let includedPurpose: CapabilityPurpose | null = null;
          const permissions = authority.permissionsByAssignment.get(assignment.id);
          const context = { role: assignment.role, permissions, scopeKind: access.scope.kind,
            modules, readyModules: ready, beta: authority.beta, capabilities: purchasedCapabilities(facts.grants, candidate.companyId, "history", ports.now(), facts.history) };
          if (definition.shell === "front" && access.scope.kind !== "department" &&
              definition.modules.some(id => modules.includes(id) && ready.includes(id)) &&
              hasPermission({ role: assignment.role, permissions }, definition.id + ".read")) roleLanding = definition.landing;
          else roleLanding = purchased.includes(definition.id)?productOfficeLanding(definition.id, context):null;
          if(!roleLanding&&(definition.id==="finance"||definition.id==="people")){
            const kind=definition.id==="finance"?"expense":"payroll",capability=kind==="expense"?"people.expenses":"people.payroll";
            if(purchasedCapabilityPurpose(facts,candidate.companyId,capability,ports.now())&&peopleFinancialPresentationAllowed({...context,modules:authority.modulesByAssignment.get(assignment.id)??[]},kind)&&Number.isSafeInteger(assignment.version)&&assignment.version!>0&&await readyModule("people_payroll")&&await readyModule("finance")){
              const source=await ports.peopleFinancialEntry?.(access,assignment,{kind});
              if(source){roleLanding=kind==="expense"?expenseFinancialLanding:payrollFinancialLanding;includedPurpose=source.purchasePurpose;}
            }
          }
          if(definition.id==="finance"&&!roleLanding&&['people.directory','people.expenses'].every(capability=>context.capabilities.includes(capability))&&expensePostingPrerequisitesPresentationAllowed({...context,modules:authority.modulesByAssignment.get(assignment.id)??[]})&&Number.isSafeInteger(assignment.version)&&assignment.version!>0&&await readyModule("people_payroll")&&await readyModule("finance")){
            const source=await ports.expensePostingPrerequisitesEntry?.(access,assignment);
            if(source){roleLanding=expensePostingPrerequisitesLanding;includedPurpose=source.purchasePurpose;}
          }
          if(definition.id==="people"&&!roleLanding&&['people.directory','people.payroll'].every(capability=>context.capabilities.includes(capability))&&payrollPostingPrerequisitesPresentationAllowed({...context,modules:authority.modulesByAssignment.get(assignment.id)??[]})&&Number.isSafeInteger(assignment.version)&&assignment.version!>0&&await readyModule("people_payroll")&&await readyModule("finance")){
            const source=await ports.payrollPostingPrerequisitesEntry?.(access,assignment);
            if(source){roleLanding=payrollPostingPrerequisitesLanding;includedPurpose=source.purchasePurpose;}
          }
          if(definition.id==="learn"&&!roleLanding&&learningWorkerPresentationAllowed(context)&&Number.isSafeInteger(assignment.version)&&assignment.version!>0&&await readyModule("learning")&&await readyModule("people_payroll")){
            const source=await ports.learningWorkerStartupEntry?.(access,assignment);
            if(source){roleLanding=learningWorkerStartupLanding;includedPurpose=source.purchasePurpose;}
          }
          if(definition.id==="projects"&&!roleLanding&&projectWorkerPresentationAllowed(context)&&Number.isSafeInteger(assignment.version)&&assignment.version!>0&&await readyModule("projects")&&await readyModule("people_payroll")){
            const source=await ports.projectWorkerStartupEntry?.(access,assignment);
            if(source){roleLanding=projectWorkerStartupLanding;includedPurpose=source.purchasePurpose;}
          }
          if(definition.id==="service"&&!roleLanding&&servicePartsPresentationAllowed(context)&&Number.isSafeInteger(assignment.version)&&assignment.version!>0&&await readyModule("field_service")&&await readyModule("inventory")&&await readyModule("finance")){
            const source=await ports.servicePartsSetupEntry?.(access,assignment);
            if(source){roleLanding=servicePartsSetupLanding;includedPurpose=source.purchasePurpose;}
          }
          if(definition.id==="assets"&&!roleLanding&&assetsFinancialPresentationAllowed(context)&&Number.isSafeInteger(assignment.version)&&assignment.version!>0&&await readyModule("assets")&&await readyModule("finance")){
            const source=await ports.assetsFinancialEntry?.(access,assignment);
            if(source){roleLanding=assetsFinancialLanding;includedPurpose=source.purchasePurpose;}
          }
          if(definition.id==="pos"&&!roleLanding&&posStartupPresentationAllowed(context)&&Number.isSafeInteger(assignment.version)&&assignment.version!>0&&await readyModule("pos")&&await readyModule("inventory")&&await readyModule("finance")){
            const source=await ports.posStartupEntry?.(access,assignment);
            if(source){roleLanding=posStartupLanding;includedPurpose=source.purchasePurpose;}
          }
          if(definition.id==="manufacturing"&&(!roleLanding||roleLanding===manufacturingInitialPhysicalStockCountLanding)){
            if(access.scope.kind!=="department"&&permissions?.includes("inventory.read")&&permissions.includes("inventory.prepare")&&Number.isSafeInteger(assignment.version)&&assignment.version!>0&&await readyModule("manufacturing")&&await readyModule("inventory")&&await readyModule("finance")){
              const entry=await ports.manufacturingInitialPhysicalStockCountEntry?.(access,assignment);
              if(entry){roleLanding=manufacturingInitialPhysicalStockCountLanding;includedPurpose=entry.purchasePurpose;}
              else if(roleLanding===manufacturingInitialPhysicalStockCountLanding)continue;
            }else if(roleLanding===manufacturingInitialPhysicalStockCountLanding)continue;
          }
          if(definition.id==="manufacturing"&&(!roleLanding||roleLanding===manufacturingPostingSetupLanding)){
            if(authority.beta||access.scope.kind!=="holding"||!permissions?.includes("finance.read")||!["finance.write","finance.approve"].some(permission=>permissions.includes(permission))||!Number.isSafeInteger(assignment.version)||assignment.version!<1||!await readyModule("manufacturing")||!await readyModule("inventory")||!await readyModule("finance"))continue;
            const entry=await ports.manufacturingPostingSetupEntry?.(access,assignment);
            if(!entry)continue;
            roleLanding=manufacturingPostingSetupLanding;includedPurpose=entry.purchasePurpose;
          }
          if(!roleLanding&&definition.id==="sales"&&access.scope.kind!=="department"&&permissions?.includes("inventory.read")&&permissions.includes("inventory.prepare")&&await readyModule("inventory")&&await readyModule("sales")&&await readyModule("finance")){
            const entry=await ports.initialPhysicalStockCountEntry?.(access,assignment);
            if(entry){roleLanding=initialPhysicalStockCountLanding;includedPurpose=entry.purchasePurpose;}
          }
          const capability = roleLanding && productEntryCapability(definition.id,roleLanding);
          const purpose = includedPurpose ?? (capability ? purchasedCapabilityPurpose(facts,candidate.companyId,capability,ports.now()) : null);
          if (!purpose) continue;
          const priority: CapabilityPurpose[] = ["new-work","settle-existing","history"];
          if (!purchasePurpose || priority.indexOf(purpose)<priority.indexOf(purchasePurpose)) { landing=roleLanding;purchasePurpose=purpose;sourceRecord=includedPurpose!==null; }
          if (purchasePurpose === "new-work") break;
        }
        if (!landing || !purchasePurpose) continue;
        let target: string;
        if (definition.shell === "front" && landing === definition.landing) {
          const slug = await ports.companySlug(access);
          if (!slug) continue;
          const scope = candidate.scope;
          const location = scope.kind === "holding" ? "holding"
            : scope.kind === "branch" ? `branches/${encodeURIComponent(scope.branchSlug)}`
              : `${scope.branchSlug ? `branches/${encodeURIComponent(scope.branchSlug)}/` : ""}departments/${encodeURIComponent(scope.departmentSlug)}`;
          target = `/${encodeURIComponent(scope.holdingSlug)}/${encodeURIComponent(slug)}/${location}/`;
        } else {
          const select = new URL("/launch", origin);
          select.searchParams.set("companyId", candidate.companyId);
          // The destination host chooses a landing for its current selected role.
          // Another applicable role's discovery must never select that role.
          select.searchParams.set("next", `${workspaceBase(candidate.scope)}/${definition.shell === "front" || landing===expenseFinancialLanding || landing===payrollFinancialLanding || landing===payrollPostingPrerequisitesLanding || landing===expensePostingPrerequisitesLanding ? landing : definition.landing}`);
          target = select.pathname + select.search;
        }
        const href = new URL("/sso/start", origin);
        href.searchParams.set("next", target);
        result.push({ ...candidate, productId: definition.id, href: href.href, purchasePurpose, ...(sourceRecord?{sourceRecord:true as const}:{}) });
      }
    }
    return result;
  };
}
