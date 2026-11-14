import {expensePostingPrerequisitesEntry,expensePostingPrerequisitesPresentationAllowed} from '@erpx/server-finance/public/rules/expense-posting-prerequisites-entry';
import {payrollPostingPrerequisitesEntry,payrollPostingPrerequisitesPresentationAllowed} from '@erpx/server-finance/public/rules/payroll-posting-prerequisites-entry';
import {peopleFinancialEntry,peopleFinancialPresentationAllowed,type PeopleFinancialEntry} from '@erpx/server-people_payroll/public/rules/financial-product-entry';
import {learningWorkerStartupLanding,learningWorkerStartupEntry,learningWorkerPresentationAllowed,type LearningWorkerStartupEntry} from "@erpx/server-people_payroll/public/rules/learning-worker-startup-entry";
import {projectWorkerStartupLanding,projectWorkerStartupEntry,projectWorkerPresentationAllowed,type ProjectWorkerStartupEntry} from "@erpx/server-people_payroll/public/rules/project-worker-startup-entry";
import {servicePartsSetupLanding,servicePartsSetupEntry,servicePartsPresentationAllowed} from "@erpx/server-field_service/public/rules/parts-entry";
import {serviceInitialPhysicalStockCountEntry,type ServiceInitialPhysicalStockCountEntry} from "@erpx/server-inventory/public/rules/service-initial-physical-stock-count";
import {posInitialPhysicalStockCountEntry,type PosInitialPhysicalStockCountEntry} from "@erpx/server-inventory/public/rules/pos-initial-physical-stock-count";
import {assetsFinancialLanding,assetsFinancialEntry,assetsFinancialPresentationAllowed,type AssetsFinancialEntry} from "@erpx/server-finance/public/rules/assets-financial-entry";
import {posStartupLanding,posStartupEntry,posStartupPresentationAllowed} from "@erpx/server-pos/public/rules/startup-entry";
import {manufacturingInitialPhysicalStockCountLanding,manufacturingInitialPhysicalStockCountEntry,type ManufacturingInitialPhysicalStockCountEntry} from "@erpx/server-inventory/public/rules/manufacturing-initial-physical-stock-count";
import {manufacturingPostingSetupLanding,manufacturingPostingSetupEntry} from "@erpx/server-manufacturing/public/rules/posting-setup-entry";
import {initialPhysicalStockCountLanding,initialPhysicalStockCountEntry,type InitialPhysicalStockCountEntry} from "@erpx/server-inventory/public/rules/initial-physical-stock-count";
import {workforceSelfServiceEntry,type WorkforceSelfServiceEntry} from '@erpx/server-people_payroll/public/rules/self-service-product-entry';
import {socialPostSourceEntry,type SocialPostSourceEntry} from '@erpx/server-collaboration/public/rules/product-entry';
import {financeDocumentEntry,salesCustomerReceiptEntry,type FinanceDocumentEntry} from "@erpx/server-finance/public/rules/document-entry";
import {qualitySourceEntry,type QualitySourceEntry} from "@erpx/server-quality/public/rules/product-entry";
import {projectTaskSourceEntry,type ProjectTaskSourceEntry} from "@erpx/server-projects/public/rules/product-entry";
import {talentHireSourceEntry,talentCandidatePrivacySourceEntry} from "@erpx/server-talent/public/rules/product-entry";
import { parseHoldingWorkspace, workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { product, type ProductHostKey } from "@erpx/server-commercial/public/rules/product-registry";
import { visiblePurchasedProducts, purchasedCapabilities, purchasedCapabilityPurpose, type CompanyPurchaseFacts, type CapabilityPurpose } from "@erpx/server-commercial/public/rules/packages";
import { productEntryCapability } from "@erpx/server-commercial/public/rules/product-entry-capability";
import { productOfficeLanding, productDestinationAvailable } from "@erpx/server-work_projection/public/rules/product-landings";
import type { ActiveWorkSelection } from "@erpx/server-work_projection/public/work-role-store";
import type { AccountAccessContext, WorkAssignment } from "@erpx/server-identity_access/domain/authorization";
import type { ProductModuleAccess } from "@erpx/server-identity_access/domain/product-module-access";
import { assignmentApplies } from "@erpx/server-identity_access/domain/workspace-access";
import { productWorkspaceTarget } from "@erpx/server-identity_access/domain/product-workspace";

export type ProductWorkspaceInput = NonNullable<ReturnType<typeof productWorkspaceTarget>>;
export type ProductWorkRoleOption = { assignmentId: string; assignmentVersion: number; role: string; label: string; target: string; purchasePurpose: CapabilityPurpose; sourceRecord?:true };
export function createProductWorkspace(ports: {
  access(input: ProductWorkspaceInput): Promise<AccountAccessContext>;
  modules(access: AccountAccessContext): Promise<ProductModuleAccess>;
  purchaseFacts(access: AccountAccessContext): Promise<CompanyPurchaseFacts>;
  commercialAdmissionDeferred?(id:NonNullable<ReturnType<typeof product>>['id']):boolean;
  ready(access: AccountAccessContext, moduleId: string): Promise<boolean>;
  executiveVisible?(access: AccountAccessContext, assignment: WorkAssignment): Promise<boolean>;
  expensePostingPrerequisitesEntry?(access:AccountAccessContext,assignment:WorkAssignment):Promise<{revision:string;purchasePurpose:"history"}|null>;
  payrollPostingPrerequisitesEntry?(access:AccountAccessContext,assignment:WorkAssignment):Promise<{revision:string;purchasePurpose:"history"}|null>;
  peopleFinancialEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:PeopleFinancialEntry):Promise<{revision:string;purchasePurpose:"history"}|null>;
  assetsFinancialEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:AssetsFinancialEntry):Promise<{revision:string;purchasePurpose:"history"}|null>;
  posInitialPhysicalStockCountEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:PosInitialPhysicalStockCountEntry):Promise<{capability:string;purchasePurpose:"history"}|null>;
  learningWorkerStartupEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:LearningWorkerStartupEntry):Promise<{revision:string;purchasePurpose:"history"}|null>;
  projectWorkerStartupEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:ProjectWorkerStartupEntry):Promise<{revision:string;purchasePurpose:"history"}|null>;
  servicePartsSetupEntry?(access:AccountAccessContext,assignment:WorkAssignment):Promise<{revision:string;purchasePurpose:"history"}|null>;
  serviceInitialPhysicalStockCountEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:ServiceInitialPhysicalStockCountEntry):Promise<{capability:string;purchasePurpose:"history"}|null>;
  posStartupEntry?(access:AccountAccessContext,assignment:WorkAssignment):Promise<{revision:string;purchasePurpose:"history"}|null>;
  manufacturingPostingSetupEntry?(access:AccountAccessContext,assignment:WorkAssignment):Promise<{revision:string;purchasePurpose:"history"}|null>;
  financeLegalSetupEntry?(access:AccountAccessContext,assignment:WorkAssignment):Promise<{revision:string}|null>;
  commercePostingSetupEntry?(access:AccountAccessContext,assignment:WorkAssignment):Promise<{revision:string;purchasePurpose:"history"}|null>;
  manufacturingInitialPhysicalStockCountEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:ManufacturingInitialPhysicalStockCountEntry):Promise<{capability:string;purchasePurpose:"history"}|null>;
  initialPhysicalStockCountEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:InitialPhysicalStockCountEntry):Promise<{capability:string;purchasePurpose:"history"}|null>;
  salesPostingSetupEntry?(access:AccountAccessContext,assignment:WorkAssignment):Promise<{revision:string;purchasePurpose:"history"}|null>;
  salesCustomerReceiptEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:{id:string}):Promise<{capability:string;purchasePurpose:"history"}|null>;
  financeDocumentEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:FinanceDocumentEntry):Promise<{capability:string;purchasePurpose:"history"}|null>;
  qualitySourceEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:QualitySourceEntry):Promise<{capability:string;purchasePurpose:"history"}|null>;
  projectTaskSourceEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:ProjectTaskSourceEntry):Promise<{capability:string;purchasePurpose:"history"}|null>;
  talentHireSourceEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:{id:string}):Promise<{capability:string;purchasePurpose:"history"}|null>;
  talentCandidatePrivacySourceEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:{id:string}):Promise<{capability:string;purchasePurpose:"history"}|null>;
  workforceSelfServiceEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:WorkforceSelfServiceEntry):Promise<{capability:string;purchasePurpose:"history"}|null>;
  socialPostSourceEntry?(access:AccountAccessContext,assignment:WorkAssignment,target:SocialPostSourceEntry):Promise<{purchasePurpose:"history"}|null>;
  selection(access: AccountAccessContext, applicable: WorkAssignment[]): Promise<ActiveWorkSelection | null>;
  select(access: AccountAccessContext, option: ProductWorkRoleOption, revision: number): Promise<ActiveWorkSelection>;
  now(): number;
  error(status: number, code: string, message: string): Error;
}) {
  async function read(audience: ProductHostKey, input: ProductWorkspaceInput) {
    const definition = product(audience);
    if (!definition) throw ports.error(400, "INVALID_PRODUCT_WORKSPACE", "Select a product workspace.");
    const validated = productWorkspaceTarget(audience, input.companyId, input.target);
    if (!validated || workspaceBase(validated.scope) !== workspaceBase(input.scope))
      throw ports.error(400, "INVALID_PRODUCT_WORKSPACE", "Select a valid product workspace.");
    const parts = new URL(input.target, "https://entry.invalid").pathname.split("/").filter(Boolean).map(decodeURIComponent);
    const destination = parseHoldingWorkspace(parts[1], parts.slice(2))!.destination.join("/");
    const access = await ports.access(input);
    if (access.expiresAt <= ports.now()) throw ports.error(401, "SESSION_EXPIRED", "Sign in again to continue.");
    if (access.scope.companyId !== input.companyId) throw ports.error(403, "COMPANY_REQUIRED", "Select an authorized Company.");
    const applicable = access.assignments.filter(assignment => assignmentApplies(assignment, access.scope));
    if (!applicable.length) throw ports.error(403, "ASSIGNMENT_UNAVAILABLE", "Your account cannot open this workspace.");
    const facts = await ports.purchaseFacts(access);
    const commercialAdmissionDeferred=ports.commercialAdmissionDeferred?.(definition.id)===true;
    const retainedExpensePrerequisites=definition.id==="finance"&&expensePostingPrerequisitesEntry(destination);
    const retainedPayrollPrerequisites=definition.id==="people"&&payrollPostingPrerequisitesEntry(destination);
    const financialTarget=peopleFinancialEntry(destination);
    const retainedFinancial=financialTarget&&((definition.id==="finance"&&financialTarget.kind==="expense")||(definition.id==="people"&&financialTarget.kind==="payroll"))&&purchasedCapabilityPurpose(facts,input.companyId,financialTarget.kind==="expense"?"people.expenses":"people.payroll",ports.now())?financialTarget:null;
    const retainedLearningWorker=definition.id==="learn"?learningWorkerStartupEntry(destination):null;
    const retainedProjectWorker=definition.id==="projects"?projectWorkerStartupEntry(destination):null;
    const retainedServiceParts=definition.id==="service"&&servicePartsSetupEntry(destination);
    const retainedServiceStock=definition.id==="service"?serviceInitialPhysicalStockCountEntry(destination):null;
    const retainedAssetsFinancial=definition.id==="assets"?assetsFinancialEntry(destination):null;
    const retainedCustomerReceipt=definition.id==="finance"?salesCustomerReceiptEntry(destination):null;
    const retainedDocument=definition.id==="finance"?financeDocumentEntry(destination):null;
    const retainedQuality=definition.id==="supply"?qualitySourceEntry(destination):null;
    const retainedSelf=definition.id==="apps"?workforceSelfServiceEntry(destination):null;
    const retainedPost=definition.id==="apps"?socialPostSourceEntry(destination):null;
    const retainedTask=definition.id==="apps"?projectTaskSourceEntry(destination):null;
    const retainedHire=definition.id==="people"?talentHireSourceEntry(destination):null;
    const retainedCandidatePrivacy=definition.id==="recruit"?talentCandidatePrivacySourceEntry(destination):null;
    const retainedLegalSetup=definition.id==="finance"&&destination==="finance/setup/thai-tax";
    const retainedPostingSetup=definition.id==="supply"&&destination==="purchasing/setup/posting";
    const retainedManufacturingInitialStock=definition.id==="manufacturing"?manufacturingInitialPhysicalStockCountEntry(destination):null;
    const retainedPosInitialStock=definition.id==="pos"?posInitialPhysicalStockCountEntry(destination):null;
    const retainedPosStartup=definition.id==="pos"&&posStartupEntry(destination);
    const retainedManufacturingPostingSetup=definition.id==="manufacturing"&&manufacturingPostingSetupEntry(destination);
    const retainedInitialPhysicalStock=definition.id==="sales"?initialPhysicalStockCountEntry(destination):null;
    const retainedSalesPostingSetup=definition.id==="sales"&&destination==="customers-sales/sales/settings/posting";
    const retainedNotifications=definition.id==="apps"&&destination==="inbox/notifications"&&purchasedCapabilities(facts.grants,input.companyId,"history",ports.now(),facts.history).includes("foundation.notifications");
    const visibleProduct=commercialAdmissionDeferred||visiblePurchasedProducts(facts.grants, input.companyId, ports.now(), "history", facts.history).includes(definition.id);
    if (!visibleProduct&&!retainedExpensePrerequisites&&!retainedPayrollPrerequisites&&!retainedFinancial&&!retainedLearningWorker&&!retainedProjectWorker&&!retainedServiceParts&&!retainedServiceStock&&!retainedPosInitialStock&&!retainedAssetsFinancial&&!retainedManufacturingInitialStock&&!retainedManufacturingPostingSetup&&!retainedInitialPhysicalStock&&!retainedCustomerReceipt&&!retainedDocument&&!retainedLegalSetup&&!retainedPostingSetup&&!retainedSalesPostingSetup&&!retainedNotifications&&!retainedQuality&&!retainedTask&&!retainedHire&&!retainedCandidatePrivacy&&!retainedPost&&!retainedSelf)
      throw ports.error(403, "PRODUCT_PACKAGE_REQUIRED", "This Company has no available purchase or retained history for this product.");
    const authority = await ports.modules(access), readiness = new Map<string, Promise<boolean>>();
    const ready = (id: string) => {
      let pending = readiness.get(id);
      if (!pending) { pending = ports.ready(access, id); readiness.set(id, pending); }
      return pending;
    };
    const options: ProductWorkRoleOption[] = [];
    for (const assignment of applicable) {
      if (!authority.modulesByAssignment.has(assignment.id) || !authority.permissionsByAssignment.has(assignment.id) ||
        !Number.isSafeInteger(assignment.version) || assignment.version! < 1) continue;
      const modules = authority.presentationModulesByAssignment.get(assignment.id) ?? [];
      const readyModules = (await Promise.all([...new Set([...modules, ...(definition.id === "apps" ? ["operations",...(retainedPost?["collaboration"]:[]),...(destination === "insights/executive" || destination.startsWith("insights/executive/") ? ["executive"] : [])] : [])])]
        .map(async id => await ready(id) ? id : null))).filter((id): id is string => id !== null);
      const executiveDestination = definition.id === "apps" && (destination === "insights/executive" || destination.startsWith("insights/executive/"));
      const canSeeExecutive = executiveDestination ? await ports.executiveVisible?.(access, assignment) ?? false : undefined;
      const context = { role: assignment.role, permissions: authority.permissionsByAssignment.get(assignment.id),
        modules, readyModules, beta: authority.beta, scopeKind: access.scope.kind, commercialAdmissionDeferred,
        capabilities: purchasedCapabilities(facts.grants, input.companyId, "history", ports.now(), facts.history),
        ...(executiveDestination ? { executiveVisible: canSeeExecutive } : {}) };
      if(retainedExpensePrerequisites){
        if(!['people.directory','people.expenses'].every(capability=>context.capabilities.includes(capability))||!expensePostingPrerequisitesPresentationAllowed({...context,modules:authority.modulesByAssignment.get(assignment.id)??[]})||!await ready("people_payroll")||!await ready("finance"))continue;
        const source=await ports.expensePostingPrerequisitesEntry?.(access,assignment);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedPayrollPrerequisites){
        if(!['people.directory','people.payroll'].every(capability=>context.capabilities.includes(capability))||!payrollPostingPrerequisitesPresentationAllowed({...context,modules:authority.modulesByAssignment.get(assignment.id)??[]})||!await ready("people_payroll")||!await ready("finance"))continue;
        const source=await ports.payrollPostingPrerequisitesEntry?.(access,assignment);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedFinancial&&(retainedFinancial.kind==="expense"||!productDestinationAvailable(definition.id,destination,context))){
        if(!context.capabilities.includes(retainedFinancial.kind==="expense"?"people.expenses":"people.payroll")||!peopleFinancialPresentationAllowed({...context,modules:authority.modulesByAssignment.get(assignment.id)??[]},retainedFinancial.kind)||!await ready("people_payroll")||!await ready("finance"))continue;
        const source=await ports.peopleFinancialEntry?.(access,assignment,retainedFinancial);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedLearningWorker){
        if(!learningWorkerPresentationAllowed(context)||!await ready("learning")||!await ready("people_payroll"))continue;
        const source=await ports.learningWorkerStartupEntry?.(access,assignment,retainedLearningWorker);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedProjectWorker){
        if(!projectWorkerPresentationAllowed(context)||!await ready("projects")||!await ready("people_payroll"))continue;
        const source=await ports.projectWorkerStartupEntry?.(access,assignment,retainedProjectWorker);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedServiceParts){
        if(!servicePartsPresentationAllowed(context)||!await ready("field_service")||!await ready("inventory")||!await ready("finance"))continue;
        const source=await ports.servicePartsSetupEntry?.(access,assignment);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedServiceStock){
        if(context.beta||access.scope.kind==="department"||!context.permissions?.includes("inventory.read")||!await ready("field_service")||!await ready("inventory")||!await ready("finance"))continue;
        const source=await ports.serviceInitialPhysicalStockCountEntry?.(access,assignment,retainedServiceStock);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedAssetsFinancial){
        if(!assetsFinancialPresentationAllowed(context)||!await ready("assets")||!await ready("finance"))continue;
        const source=await ports.assetsFinancialEntry?.(access,assignment,retainedAssetsFinancial);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedPosInitialStock){
        if(context.beta||access.scope.kind==="department"||!context.permissions?.includes("inventory.read")||!await ready("pos")||!await ready("inventory")||!await ready("finance"))continue;
        const source=await ports.posInitialPhysicalStockCountEntry?.(access,assignment,retainedPosInitialStock);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      const namedPosStartup=retainedPosStartup&&(destination===posStartupLanding||!context.permissions?.includes("pos.read"));
      if(definition.shell==="front"&&!definition.modules.some(id=>modules.includes(id)&&readyModules.includes(id))&&!namedPosStartup)continue;
      if(namedPosStartup){
        if(!posStartupPresentationAllowed(context,destination!=="pos/settings")||!await ready("pos")||!await ready("inventory")||!await ready("finance"))continue;
        const source=await ports.posStartupEntry?.(access,assignment);
        if(source){const url=new URL(input.target,"https://entry.invalid");options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:url.pathname+url.search+url.hash,purchasePurpose:source.purchasePurpose,sourceRecord:true});}
        continue;
      }
      if(retainedNotifications&&!visibleProduct){
        if(await ready("collaboration"))options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:"history",sourceRecord:true});
        continue;
      }
      if(retainedSelf){
        if(!await ready("people_payroll"))continue;
        const source=await ports.workforceSelfServiceEntry?.(access,assignment,retainedSelf);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedPost&&(!visibleProduct||!productDestinationAvailable(definition.id,destination,context))){
        if(!await ready("collaboration"))continue;
        const source=await ports.socialPostSourceEntry?.(access,assignment,retainedPost);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedTask&&(!visibleProduct||!productDestinationAvailable(definition.id,destination,context))){
        if(!context.permissions?.includes("records.read")||!await ready("projects")||!await ready("operations"))continue;
        const source=await ports.projectTaskSourceEntry?.(access,assignment,retainedTask);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedHire&&(!visibleProduct||!productDestinationAvailable(definition.id,destination,context))){
        if(access.scope.kind!=="holding"||!context.permissions?.includes("people.manage")||!await ready("people_payroll")||!await ready("talent"))continue;
        const source=await ports.talentHireSourceEntry?.(access,assignment,retainedHire);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedCandidatePrivacy&&(!visibleProduct||!productDestinationAvailable(definition.id,destination,context))){
        if(access.scope.kind!=="holding"||!["legal.hold","legal.hold.release","legal.destroy","people.manage"].some(permission=>context.permissions?.includes(permission))||!await ready("talent"))continue;
        const source=await ports.talentCandidatePrivacySourceEntry?.(access,assignment,retainedCandidatePrivacy);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      // Actual receipt/return quarantine and its linked disposal Count retain
      // only their owner-proven source entry. Buying another product cannot
      // discover Supply, its collections, or a different selected role.
      if(retainedQuality&&(!visibleProduct||!productDestinationAvailable(definition.id,destination,context))){
        const permission=retainedQuality.kind==="hold"?"quality.read":"inventory.read";
        if(!context.permissions?.includes(permission)||!await ready(retainedQuality.kind==="hold"?"quality":"inventory"))continue;
        const source=await ports.qualitySourceEntry?.(access,assignment,retainedQuality);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      // The initial Company posting configuration is Finance-owned work on
      // these exact Sales/Supply pages. Owner-proven entry grants no collections,
      // discovery, modules or replacement acting role.
      if(retainedManufacturingInitialStock){
        if(access.scope.kind==="department"||!context.permissions?.includes("inventory.read")||!await ready("manufacturing")||!await ready("inventory")||!await ready("finance"))continue;
        const source=await ports.manufacturingInitialPhysicalStockCountEntry?.(access,assignment,retainedManufacturingInitialStock);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedManufacturingPostingSetup){
        if(context.beta||access.scope.kind!=="holding"||!context.permissions?.includes("finance.read")||!["finance.write","finance.approve"].some(permission=>context.permissions?.includes(permission))||!await ready("manufacturing")||!await ready("inventory")||!await ready("finance"))continue;
        const source=await ports.manufacturingPostingSetupEntry?.(access,assignment);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedInitialPhysicalStock){
        if(access.scope.kind==="department"||!context.permissions?.includes("inventory.read")||!await ready("inventory")||!await ready("sales")||!await ready("finance"))continue;
        const source=await ports.initialPhysicalStockCountEntry?.(access,assignment,retainedInitialPhysicalStock);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedSalesPostingSetup&&(!visibleProduct||!productDestinationAvailable(definition.id,destination,context))){
        if(access.scope.kind!=="holding"||!context.permissions?.includes("finance.read")||!["finance.write","finance.approve"].some(permission=>context.permissions?.includes(permission))||!await ready("finance")||!await ready("sales"))continue;
        const source=await ports.salesPostingSetupEntry?.(access,assignment);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedPostingSetup&&(!visibleProduct||!productDestinationAvailable(definition.id,destination,context))){
        if(access.scope.kind!=="holding"||!context.permissions?.includes("finance.read")||!["finance.write","finance.approve"].some(permission=>context.permissions?.includes(permission))||!await ready("finance")||!await ready("procurement"))continue;
        const source=await ports.commercePostingSetupEntry?.(access,assignment);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      if(retainedCustomerReceipt&&(!visibleProduct||!productDestinationAvailable(definition.id,destination,context))){
        if(!context.permissions?.includes("finance.read")||!await ready("finance"))continue;
        const source=await ports.salesCustomerReceiptEntry?.(access,assignment,retainedCustomerReceipt);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:source.purchasePurpose,sourceRecord:true});
        continue;
      }
      // A real source record can be visited without discovering the Finance
      // product. Original Finance permission, release, owner identity,
      // purchase and current assignment are still independently required.
      if((retainedDocument||retainedLegalSetup)&&(!visibleProduct||!productDestinationAvailable(definition.id,destination,context))){
        if(!context.permissions?.includes("finance.read")||!await ready("finance"))continue;
        const source=retainedDocument?await ports.financeDocumentEntry?.(access,assignment,retainedDocument)
          : await ports.financeLegalSetupEntry?.(access,assignment);
        if(source)options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:input.target,purchasePurpose:"purchasePurpose" in source?source.purchasePurpose:"history",sourceRecord:true});
        continue;
      }
      if(!visibleProduct)continue;
      const landing = definition.shell === "front" ? destination : productOfficeLanding(definition.id, context);
      if(!landing&&definition.id==="learn"&&destination===definition.landing){
        if(!learningWorkerPresentationAllowed(context)||!await ready("learning")||!await ready("people_payroll"))continue;
        const source=await ports.learningWorkerStartupEntry?.(access,assignment,{});
        if(source){const url=new URL(input.target,"https://entry.invalid");url.pathname=`${workspaceBase(input.scope)}/${learningWorkerStartupLanding}`;options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:url.pathname+url.search+url.hash,purchasePurpose:source.purchasePurpose,sourceRecord:true});}
        continue;
      }
      if(!landing&&definition.id==="projects"&&destination===definition.landing){
        if(!projectWorkerPresentationAllowed(context)||!await ready("projects")||!await ready("people_payroll"))continue;
        const source=await ports.projectWorkerStartupEntry?.(access,assignment,{});
        if(source){const url=new URL(input.target,"https://entry.invalid");url.pathname=`${workspaceBase(input.scope)}/${projectWorkerStartupLanding}`;options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:url.pathname+url.search+url.hash,purchasePurpose:source.purchasePurpose,sourceRecord:true});}
        continue;
      }
      if(!landing&&definition.id==="service"&&destination===definition.landing){
        if(!servicePartsPresentationAllowed(context)||!await ready("field_service")||!await ready("inventory")||!await ready("finance"))continue;
        const source=await ports.servicePartsSetupEntry?.(access,assignment);
        if(source){const url=new URL(input.target,"https://entry.invalid");url.pathname=`${workspaceBase(input.scope)}/${servicePartsSetupLanding}`;options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:url.pathname+url.search+url.hash,purchasePurpose:source.purchasePurpose,sourceRecord:true});}
        continue;
      }
      if(!landing&&definition.id==="assets"&&destination===definition.landing){
        if(!assetsFinancialPresentationAllowed(context)||!await ready("assets")||!await ready("finance"))continue;
        const source=await ports.assetsFinancialEntry?.(access,assignment,{});
        if(source){const url=new URL(input.target,"https://entry.invalid");url.pathname=`${workspaceBase(input.scope)}/${assetsFinancialLanding}`;options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:url.pathname+url.search+url.hash,purchasePurpose:source.purchasePurpose,sourceRecord:true});}
        continue;
      }
      if(definition.id==="manufacturing"&&destination===definition.landing&&(!landing||landing===manufacturingInitialPhysicalStockCountLanding)){
        if(access.scope.kind!=="department"&&context.permissions?.includes("inventory.read")&&context.permissions.includes("inventory.prepare")&&await ready("manufacturing")&&await ready("inventory")&&await ready("finance")){
          const source=await ports.manufacturingInitialPhysicalStockCountEntry?.(access,assignment,{});
          if(source){const url=new URL(input.target,"https://entry.invalid");url.pathname=`${workspaceBase(input.scope)}/${manufacturingInitialPhysicalStockCountLanding}`;options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:url.pathname+url.search+url.hash,purchasePurpose:source.purchasePurpose,sourceRecord:true});continue;}
        }
        if(landing===manufacturingInitialPhysicalStockCountLanding)continue;
      }
      if((!landing||landing===manufacturingPostingSetupLanding)&&definition.id==="manufacturing"&&destination===definition.landing){
        if(context.beta||access.scope.kind!=="holding"||!context.permissions?.includes("finance.read")||!["finance.write","finance.approve"].some(permission=>context.permissions?.includes(permission))||!await ready("manufacturing")||!await ready("inventory")||!await ready("finance"))continue;
        const source=await ports.manufacturingPostingSetupEntry?.(access,assignment);
        if(source){const url=new URL(input.target,"https://entry.invalid");url.pathname=`${workspaceBase(input.scope)}/${manufacturingPostingSetupLanding}`;options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:url.pathname+url.search+url.hash,purchasePurpose:source.purchasePurpose,sourceRecord:true});}
        continue;
      }
      if(!landing&&definition.id==="sales"&&destination===definition.landing&&access.scope.kind!=="department"&&context.permissions?.includes("inventory.read")&&context.permissions.includes("inventory.prepare")&&await ready("inventory")&&await ready("sales")&&await ready("finance")){
        const source=await ports.initialPhysicalStockCountEntry?.(access,assignment,{});
        if(source){const url=new URL(input.target,"https://entry.invalid");url.pathname=`${workspaceBase(input.scope)}/${initialPhysicalStockCountLanding}`;options.push({assignmentId:assignment.id,assignmentVersion:assignment.version!,role:assignment.role,label:assignment.label??assignment.role.replaceAll("_"," "),target:url.pathname+url.search+url.hash,purchasePurpose:source.purchasePurpose,sourceRecord:true});}
        continue;
      }
      if (!landing) continue;
      let target = input.target;
      if (destination === definition.landing) {
        const url = new URL(input.target, "https://entry.invalid");
        url.pathname = `${workspaceBase(input.scope)}/${landing}`;
        target = url.pathname + url.search + url.hash;
      } else if (!productDestinationAvailable(definition.id, destination, context)) continue;
      const capability = productEntryCapability(definition.id,destination===definition.landing?landing:destination);
      const purchasePurpose = capability ? commercialAdmissionDeferred ? 'new-work' : purchasedCapabilityPurpose(facts,input.companyId,capability,ports.now()) : null;
      if (!purchasePurpose) continue;
      options.push({ assignmentId: assignment.id, assignmentVersion: assignment.version!, role: assignment.role,
        label: assignment.label ?? assignment.role.replaceAll("_", " "), target, purchasePurpose });
    }
    if (!options.length) throw ports.error(403, "PRODUCT_ROLE_REQUIRED", "None of your current roles can open this product page in this workspace.");
    // Normal initialization can restore a saved role or initialize a single
    // assignment. It never replaces an existing live session selection.
    const selection = await ports.selection(access, applicable);
    const selected = selection && selection.expires_at > ports.now() ? options.find(option =>
      option.assignmentId === selection.assignment_id && option.assignmentVersion === selection.assignment_version) : undefined;
    return { access, options, selection, target: selected?.target ?? null, sourceRecord:selected?.sourceRecord===true };
  }
  return {
    read,
    async choose(audience: ProductHostKey, input: ProductWorkspaceInput, assignmentId: string, revision: number, assignmentVersion: number) {
      const state = await read(audience, input), option = state.options.find(option => option.assignmentId === assignmentId);
      if (!option) throw ports.error(403, "ASSIGNMENT_UNAVAILABLE", "Choose one of the currently available work roles.");
      if (option.assignmentVersion !== assignmentVersion) throw ports.error(409, "WORK_CONTEXT_CHANGED", "This work role changed. Review the current choices.");
      if ((state.selection?.revision ?? 0) !== revision) throw ports.error(409, "WORK_CONTEXT_CHANGED", "Your work role changed. Review the current choices.");
      if (state.target && state.selection?.assignment_id === option.assignmentId &&
        state.selection.assignment_version === option.assignmentVersion && state.selection.expires_at > ports.now()) return state;
      await ports.select(state.access, option, revision);
      // The subsequent GET rechecks purchase, release, authority and the selected
      // context; a POST response never redirects a mutation to another host.
      return state;
    },
  };
}
