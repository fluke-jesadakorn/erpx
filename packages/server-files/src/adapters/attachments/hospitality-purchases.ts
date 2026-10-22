import {messagingAttachmentCommandBoundary,type MessagingAttachmentPurchase} from "@erpx/server-collaboration/public/messaging-attachment-purchases";
import {ecommerceAttachmentCommandBoundary,type EcommerceAttachmentPurchase} from "@erpx/server-ecommerce/public/attachment-purchases";
import {workTaskAttachmentCommandBoundary,type WorkTaskAttachmentPurchase} from "@erpx/server-operations/public/task-attachment-purchases";
import {portalAttachmentCommandBoundary,type PortalAttachmentPurchase} from "@erpx/server-customer_authority/public/attachment-purchases";
import {serviceAttachmentCommandBoundary,type ServiceAttachmentPurchase} from "@erpx/server-field_service/public/attachment-purchases";
import {benefitAttachmentCommandBoundary,type BenefitAttachmentPurchase} from "@erpx/server-benefits/public/attachment-purchases";
import {workforceAttachmentCommandBoundary,type WorkforceAttachmentPurchase} from "@erpx/server-people_payroll/public/workforce-attachment-purchases";
import {crmAttachmentCommandBoundary,type CRMAttachmentPurchase} from "@erpx/server-crm/public/attachment-purchases";
import {marketingAttachmentCommandBoundary,type MarketingAttachmentPurchase} from "@erpx/server-marketing/public/attachment-purchases";
import {manufacturingAttachmentCommandBoundary,type ManufacturingAttachmentPurchase} from "@erpx/server-manufacturing/public/attachment-purchases";
import {legalAttachmentCommandBoundary,type LegalAttachmentPurchase} from "@erpx/server-legal/public/attachment-purchases";
import {talentAttachmentCommandBoundary,type TalentAttachmentPurchase} from "@erpx/server-talent/public/attachment-purchases";
import {landedCostAttachmentCommandBoundary,type LandedCostAttachmentPurchase} from "@erpx/server-inventory/public/landed-cost-attachment-purchases";
import {assetAttachmentCommandBoundary,type AssetAttachmentPurchase} from "@erpx/server-assets/public/attachment-purchases";
import {learningAttachmentCommandBoundary,type LearningAttachmentPurchase} from "@erpx/server-learning/public/attachment-purchases";
import {qualityAttachmentCommandBoundary,type QualityAttachmentPurchase} from "@erpx/server-quality/public/attachment-purchases";
import {projectAttachmentCommandBoundary,type ProjectAttachmentPurchase} from "@erpx/server-projects/public/attachment-purchases";
import {transferAttachmentCommandBoundary,type TransferAttachmentPurchase} from "@erpx/server-inventory/public/transfer-attachment-purchases";
import {peopleAttachmentCommandBoundary,type PeopleAttachmentPurchase} from "@erpx/server-people_payroll/public/base-attachment-purchases";
import {procurementAttachmentCommandBoundary,type ProcurementAttachmentPurchase} from "@erpx/server-procurement/public/attachment-purchases";
import {financeAttachmentCommandBoundary,type FinanceAttachmentPurchase} from "@erpx/server-finance/public/attachment-purchases";
import {commerceAttachmentCommandBoundary,type CommerceAttachmentPurchase} from "@erpx/server-commerce/public/attachment-purchases";
import {payrollAttachmentCommandBoundary,type PayrollAttachmentPurchase} from "@erpx/server-people_payroll/public/attachment-purchases";
import {countAttachmentCommandBoundary,type CountAttachmentPurchase} from "@erpx/server-inventory/public/count-attachment-purchases";
import "server-only";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { AttachmentOperation, AttachmentPolicy } from "@erpx/server-files/public/attachments/types";
import { posAttachmentPurchase } from "@erpx/server-pos/public/rules/attachment-purchases";
import { pmsAttachmentPurchase } from "@erpx/server-pms/public/rules/attachment-purchases";
import { checkPurchasedCapability, posPackageEnforcementMode, pmsPackageEnforcementMode, purchaseCommandBoundary, observePurchaseResults,companyPurchaseReadLocks } from "@erpx/server-commercial/public/purchase-command-boundary";
import { supportAttachmentCommandBoundary, type SupportAttachmentPurchase } from "@erpx/server-support/public/attachment-purchases";

export type HospitalityAttachmentPurchase = { owner: "pos" | "pms"; companyId: string; capability: string; purpose: "new-work" | "settle-existing" };
export function hospitalityAttachmentPurchaseScope(actor: WorkContext, policy: Pick<AttachmentPolicy, "targetType" | "purposeKey">): HospitalityAttachmentPurchase | null {
  const pos = posAttachmentPurchase(policy.targetType);
  if (pos) return { owner: "pos", companyId: actor.companyId, ...pos };
  const pms = pmsAttachmentPurchase(policy.targetType, policy.purposeKey);
  return pms ? { owner: "pms", companyId: actor.companyId, ...pms } : null;
}
const modeFor = (owner: HospitalityAttachmentPurchase["owner"]) => owner === "pos" ? posPackageEnforcementMode() : pmsPackageEnforcementMode();

/** Files keeps its original IAM/version/retention/scanner policy. These are
 * additional owner capabilities; reading retained evidence never grants edits. */
export async function authorizeHospitalityAttachmentPurchase(database: SqlDatabase, actor: WorkContext, policy: AttachmentPolicy, operation: AttachmentOperation) {
  const scope = hospitalityAttachmentPurchaseScope(actor, policy);
  if (scope) await checkPurchasedCapability(database, await modeFor(scope.owner), {
    companyId: scope.companyId, capability: scope.capability, purpose: operation === "read" ? "history" : scope.purpose,
  });
}

/** Both raw Files batches and its normal executeCommand use these guards in
 * their original transaction, including the final post-receipt purchase clock. */
async function hospitalityBoundary(database: SqlDatabase, scopes: readonly HospitalityAttachmentPurchase[]) {
  if (!scopes.length) return undefined;
  const unique = [...new Map(scopes.map(scope => [`${scope.companyId}:${scope.owner}:${scope.capability}:${scope.purpose}`, scope])).values()]
    .sort((a, b) => a.companyId.localeCompare(b.companyId) || a.owner.localeCompare(b.owner) || a.capability.localeCompare(b.capability) || a.purpose.localeCompare(b.purpose));
  const boundaries = [];
  for (const scope of unique) {
    const mode = await modeFor(scope.owner);
    if (mode === "disabled") continue;
    const key = `hospitality-file-purchase:${crypto.randomUUID()}`;
    boundaries.push({ key, boundary: purchaseCommandBoundary(database, mode, key, scope)! });
  }
  if (!boundaries.length) return undefined;
  return { before: boundaries.flatMap(item => item.boundary.before), after: [...boundaries.flatMap(item => item.boundary.after),
    ...boundaries.map(item => database.prepare("DELETE FROM command_assertions WHERE command_id=?").bind(item.key))],
    observe: boundaries.some(item => !!item.boundary.observe) ? observePurchaseResults : undefined };
}

export async function attachmentPurchaseBoundary(database: SqlDatabase, support: readonly SupportAttachmentPurchase[], hospitality: readonly HospitalityAttachmentPurchase[], finance:readonly FinanceAttachmentPurchase[] = [],commerce:readonly CommerceAttachmentPurchase[] = [],payroll:readonly PayrollAttachmentPurchase[] = [],counts:readonly CountAttachmentPurchase[] = [],procurement:readonly ProcurementAttachmentPurchase[] = [],basePeople:readonly PeopleAttachmentPurchase[] = [],transfers:readonly TransferAttachmentPurchase[] = [],projects:readonly ProjectAttachmentPurchase[] = [],quality:readonly QualityAttachmentPurchase[] = [],learning:readonly LearningAttachmentPurchase[] = [],assets:readonly AssetAttachmentPurchase[] = [],landedCosts:readonly LandedCostAttachmentPurchase[] = [],talent:readonly TalentAttachmentPurchase[] = [],manufacturing:readonly ManufacturingAttachmentPurchase[] = [],legal:readonly LegalAttachmentPurchase[] = [],crm:readonly CRMAttachmentPurchase[] = [],marketing:readonly MarketingAttachmentPurchase[] = [],workforce:readonly WorkforceAttachmentPurchase[] = [],benefits:readonly BenefitAttachmentPurchase[] = [],service:readonly ServiceAttachmentPurchase[] = [],portal:readonly PortalAttachmentPurchase[] = [],workTasks:readonly WorkTaskAttachmentPurchase[] = [],ecommerce:readonly EcommerceAttachmentPurchase[] = [],messaging:readonly MessagingAttachmentPurchase[] = []) {
  const original = await supportAttachmentCommandBoundary(database, support), added = await hospitalityBoundary(database, hospitality), financial=await financeAttachmentCommandBoundary(database,finance),orders=await commerceAttachmentCommandBoundary(database,commerce),people=await payrollAttachmentCommandBoundary(database,payroll),count=await countAttachmentCommandBoundary(database,counts),rfq=await procurementAttachmentCommandBoundary(database,procurement),base=await peopleAttachmentCommandBoundary(database,basePeople),transfer=await transferAttachmentCommandBoundary(database,transfers),project=await projectAttachmentCommandBoundary(database,projects),inspection=await qualityAttachmentCommandBoundary(database,quality),learner=await learningAttachmentCommandBoundary(database,learning),physical=await assetAttachmentCommandBoundary(database,assets),landed=await landedCostAttachmentCommandBoundary(database,landedCosts),recruitment=await talentAttachmentCommandBoundary(database,talent),production=await manufacturingAttachmentCommandBoundary(database,manufacturing),legalDocuments=await legalAttachmentCommandBoundary(database,legal),customers=await crmAttachmentCommandBoundary(database,crm),campaigns=await marketingAttachmentCommandBoundary(database,marketing),selfService=await workforceAttachmentCommandBoundary(database,workforce),benefitEvidence=await benefitAttachmentCommandBoundary(database,benefits),serviceEvidence=await serviceAttachmentCommandBoundary(database,service),portalEvidence=await portalAttachmentCommandBoundary(database,portal),workTaskEvidence=await workTaskAttachmentCommandBoundary(database,workTasks),ecommerceEvidence=await ecommerceAttachmentCommandBoundary(database,ecommerce),messagingEvidence=await messagingAttachmentCommandBoundary(database,messaging);
  if (!original && !added && !financial && !orders && !people && !count && !rfq && !base && !transfer && !project && !inspection && !learner && !physical && !landed && !recruitment && !production && !legalDocuments && !customers && !campaigns && !selfService && !benefitEvidence && !serviceEvidence && !portalEvidence && !workTaskEvidence && !ecommerceEvidence && !messagingEvidence) return undefined;
  const companies=[...new Set([...support,...hospitality,...finance,...commerce,...payroll,...counts,...procurement,...basePeople,...transfers,...projects,...quality,...learning,...assets,...landedCosts,...talent,...manufacturing,...legal,...crm,...marketing,...workforce,...benefits,...service,...portal,...workTasks,...ecommerce,...messaging].map(scope=>scope.companyId))].sort();
  return {before:[...companies.flatMap(company=>companyPurchaseReadLocks(database,company)),...original?.before??[],...added?.before??[],...financial?.before??[],...orders?.before??[],...people?.before??[],...count?.before??[],...rfq?.before??[],...base?.before??[],...transfer?.before??[],...project?.before??[],...inspection?.before??[],...learner?.before??[],...physical?.before??[],...landed?.before??[],...recruitment?.before??[],...production?.before??[],...legalDocuments?.before??[],...customers?.before??[],...campaigns?.before??[],...selfService?.before??[],...benefitEvidence?.before??[],...serviceEvidence?.before??[],...portalEvidence?.before??[],...workTaskEvidence?.before??[],...ecommerceEvidence?.before??[],...messagingEvidence?.before??[]],after:[...messagingEvidence?.after??[],...ecommerceEvidence?.after??[],...workTaskEvidence?.after??[],...portalEvidence?.after??[],...serviceEvidence?.after??[],...benefitEvidence?.after??[],...selfService?.after??[],...campaigns?.after??[],...customers?.after??[],...legalDocuments?.after??[],...production?.after??[],...recruitment?.after??[],...landed?.after??[],...physical?.after??[],...learner?.after??[],...inspection?.after??[],...project?.after??[],...transfer?.after??[],...base?.after??[],...financial?.after??[],...orders?.after??[],...people?.after??[],...count?.after??[],...rfq?.after??[],...original?.after??[],...added?.after??[]],observe:original?.observe||added?.observe||financial?.observe||orders?.observe||people?.observe||count?.observe||rfq?.observe||base?.observe||transfer?.observe||project?.observe||inspection?.observe||learner?.observe||physical?.observe||landed?.observe||recruitment?.observe||production?.observe||legalDocuments?.observe||customers?.observe||campaigns?.observe||selfService?.observe||benefitEvidence?.observe||serviceEvidence?.observe||portalEvidence?.observe||workTaskEvidence?.observe||ecommerceEvidence?.observe||messagingEvidence?.observe?observePurchaseResults:undefined};
}
