import {posAddonPackageObligations} from "../../pos/public/addon-package-obligations";
import {pmsAddonPackageObligations} from "../../pms/public/addon-package-obligations";
import {hospitalityAddonFinancePackageObligations,lockHospitalityAddonFinancePackageObligationSources} from "../../finance/public/hospitality-addon-package-obligations";
import {hospitalityAddonInventoryPackageObligations,lockHospitalityAddonInventoryPackageObligationSources} from "../../inventory/public/hospitality-addon-package-obligations";
import {peopleAddonPackageObligations,lockPeopleAddonPackageObligationSources} from "../../people_payroll/public/addon-package-obligations";
import {peopleAddonFinancePackageObligations,lockPeopleAddonFinancePackageObligationSources} from "../../finance/public/people-addon-package-obligations";
import {performancePackageObligations,lockPerformancePackageObligationSources} from "../../people_payroll/public/performance-package-obligations";
import {portalAuthorityPackageObligations,lockPortalAuthorityPackageObligationSources} from "../../customer_authority/public/package-obligations";
import {portalCustomerPackageObligations,lockPortalCustomerPackageObligationSources} from "../../customer_360/public/package-obligations";
import {ecommercePackageObligations,lockEcommercePackageObligationSources} from "../../ecommerce/public/package-obligations";
import {ecommerceSalesPackageObligations,lockEcommerceSalesPackageObligationSources} from "../../sales/public/ecommerce-package-obligations";
import {ecommerceInventoryPackageObligations,lockEcommerceInventoryPackageObligationSources} from "../../inventory/public/ecommerce-package-obligations";
import {ecommerceFinancePackageObligations,lockEcommerceFinancePackageObligationSources} from "../../finance/public/ecommerce-package-obligations";
import {supplyProcurementPackageObligations,lockSupplyProcurementPackageObligationSources} from "../../procurement/public/package-obligations";
import {supplyInventoryPackageObligations,lockSupplyInventoryPackageObligationSources} from "../../inventory/public/supply-package-obligations";
import {supplyQualityPackageObligations,lockSupplyQualityPackageObligationSources} from "../../quality/public/supply-package-obligations";
import {supplyFinancePackageObligations,lockSupplyFinancePackageObligationSources} from "../../finance/public/supply-package-obligations";
import {workOperationsPackageObligations,lockWorkOperationsPackageObligationSources} from "../../operations/public/work-package-obligations";
import {workCollaborationPackageObligations,lockWorkCollaborationPackageObligationSources} from "../../collaboration/public/work-package-obligations";
import {salesPackageObligations,lockSalesPackageObligationSources} from "../../sales/public/package-obligations";
import {salesCRMPackageObligations,lockSalesCRMPackageObligationSources} from "../../crm/public/sales-package-obligations";
import {salesInventoryPackageObligations,lockSalesInventoryPackageObligationSources} from "../../inventory/public/sales-package-obligations";
import {salesFinancePackageObligations,lockSalesFinancePackageObligationSources} from "../../finance/public/sales-package-obligations";
import {servicePackageObligations,lockServicePackageObligationSources} from "../../field_service/public/package-obligations";
import {serviceSupportPackageObligations,lockServiceSupportPackageObligationSources} from "../../support/public/service-package-obligations";
import {serviceInventoryPackageObligations,lockServiceInventoryPackageObligationSources} from "../../inventory/public/service-package-obligations";
import {serviceAssetPackageObligations,lockServiceAssetPackageObligationSources} from "../../assets/public/service-package-obligations";
import {serviceFinancePackageObligations,lockServiceFinancePackageObligationSources} from "../../finance/public/service-package-obligations";
import {benefitPackageObligations,lockBenefitPackageObligationSources} from "../../benefits/public/package-obligations";
import {benefitPeoplePackageObligations,lockBenefitPeoplePackageObligationSources} from "../../people_payroll/public/benefits-package-obligations";
import {benefitFinancePackageObligations,lockBenefitFinancePackageObligationSources} from "../../finance/public/benefits-package-obligations";
import {marketingPackageObligations,lockMarketingPackageObligationSources} from "../../marketing/public/package-obligations";
import {marketingCRMPackageObligations,lockMarketingCRMPackageObligationSources} from "../../crm/public/marketing-package-obligations";
import {marketingFinancePackageObligations,lockMarketingFinancePackageObligationSources} from "../../finance/public/marketing-package-obligations";
import {peoplePackageObligations,lockPeoplePackageObligationSources} from "../../people_payroll/public/package-obligations";
import {peopleBenefitsPackageObligations,lockPeopleBenefitsPackageObligationSources} from "../../benefits/public/people-package-obligations";
import {peopleFinancePackageObligations,lockPeopleFinancePackageObligationSources} from "../../finance/public/people-package-obligations";
import {assetPackageObligations,lockAssetPackageObligationSources} from "../../assets/public/package-obligations";
import {maintenancePackageObligations,lockMaintenancePackageObligationSources} from "../../maintenance/public/package-obligations";
import {assetsFinancePackageObligations,lockAssetsFinancePackageObligationSources} from "../../finance/public/assets-package-obligations";
import {projectPackageObligations,lockProjectPackageObligationSources} from "../../projects/public/package-obligations";
import {projectPeoplePackageObligations,lockProjectPeoplePackageObligationSources} from "../../people_payroll/public/project-package-obligations";
import {projectFinancePackageObligations,lockProjectFinancePackageObligationSources} from "../../finance/public/project-package-obligations";
import {legalPackageObligations,lockLegalPackageObligationSources} from "../../legal/public/package-obligations";
import {talentPackageObligations,lockTalentPackageObligationSources} from "../../talent/public/package-obligations";
import {talentPeoplePackageObligations,lockTalentPeoplePackageObligationSources} from "../../people_payroll/public/talent-package-obligations";
import {learningPackageObligations,lockLearningPackageObligationSources} from "../../learning/public/package-obligations";
import {learningPeoplePackageObligations,lockLearningPeoplePackageObligationSources} from "../../people_payroll/public/learning-package-obligations";
import {manufacturingPackageObligations,lockManufacturingPackageObligationSources} from "../../manufacturing/public/package-obligations";
import {manufacturingPlanningPackageObligations,lockManufacturingPlanningPackageObligationSources} from "../../planning/public/package-obligations";
import {manufacturingQualityPackageObligations,lockManufacturingQualityPackageObligationSources} from "../../quality/public/manufacturing-package-obligations";
import {manufacturingInventoryPackageObligations,lockManufacturingInventoryPackageObligationSources} from "../../inventory/public/manufacturing-package-obligations";
import {manufacturingFinancePackageObligations,lockManufacturingFinancePackageObligationSources} from "../../finance/public/manufacturing-package-obligations";
import { flushFinanceDeferredEffects } from "../../finance/public/finance-transactions";
import "server-only";
import type { PostgresDatabase } from "../../runtime/public/runtime/postgres";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { supportPackageObligations, lockSupportPackageObligationSources } from "../../support/public/package-obligations";
import { posPackageObligations, lockPosPackageObligationSources } from "../../pos/public/package-obligations";
import { posFinancePackageObligations, lockPosFinancePackageObligationSources } from "../../finance/public/pos-package-obligations";
import { posInventoryPackageObligations, lockPosInventoryPackageObligationSources } from "../../inventory/public/pos-package-obligations";
import { pmsPackageObligations, lockPmsPackageObligationSources } from "../../pms/public/package-obligations";
import { pmsPosPackageObligations } from "../../pos/public/pms-package-obligations";
import { pmsFinancePackageObligations, lockPmsFinancePackageObligationSources } from "../../finance/public/pms-package-obligations";
import { pmsInventoryPackageObligations, lockPmsInventoryPackageObligationSources } from "../../inventory/public/pms-package-obligations";
import { financePackageObligations, lockFinancePackageObligationSources } from "../../finance/public/package-obligations";
import { withPackageSettlementSources, type PackageSettlementSourceLock } from "./package-settlement-sources";
import { readPackageObligations, type PackageObligationInput } from "./package-obligations";
import { readCompanyPackageGrants } from "./package-grants";
import { requiresPackageSettlementReview } from "../domain/packages";

/** Lifecycle Preview/Apply composes its normal Commercial service in this unit.
 * Additional owners must supply their source locks before registering a package.
 */
type SettlementInput = Pick<PackageObligationInput, "companyId" | "packageId" | "packageVersion">;
export const hasOwnerPackageObligationSources = (input: Pick<SettlementInput, "packageId" | "packageVersion">) => input.packageVersion === 1 && ["work", "work-social", "work-messages", "work-routines", "support", "support-portal", "pos", "pos-restaurant", "pos-hotel-charge", "pms", "pms-minibar-linen", "finance", "finance-advanced", "manufacturing", "manufacturing-mrp", "recruit", "legal", "learn", "assets", "assets-preventive", "assets-parts-costs", "projects", "projects-costs", "people", "people-workforce", "people-payroll", "people-expenses", "people-performance", "sales-marketing", "people-benefits", "service", "service-parts-assets", "sales", "supply", "supply-quality", "supply-advanced", "sales-ecommerce", "sales-portal"].includes(input.packageId);
export function withOwnerPackageObligations<Result>(owner: PostgresDatabase,
  input: SettlementInput | readonly SettlementInput[],
  run: (transaction: SqlDatabase) => Promise<Result>,
  finalize?: (transaction: SqlDatabase) => Promise<void>) {
  const inputs: readonly SettlementInput[] = Array.isArray(input) ? input : [input as SettlementInput];
  if (!inputs.length || inputs.some(item => !hasOwnerPackageObligationSources(item) || !item.companyId || item.companyId !== inputs[0].companyId) ||
    new Set(inputs.map(item => item.packageId)).size !== inputs.length)
    throw new Error("PACKAGE_OBLIGATION_SOURCES_REQUIRED");
  const companyId = inputs[0].companyId, sourceLocks: ((lock: PackageSettlementSourceLock) => Promise<void>)[] = [];
  if(inputs.some(item=>["work","work-social","work-routines"].includes(item.packageId)))sourceLocks.push(lock=>lockWorkOperationsPackageObligationSources(owner,lock));
  if(inputs.some(item=>["work","work-social","work-messages","work-routines"].includes(item.packageId)))sourceLocks.push(lock=>lockWorkCollaborationPackageObligationSources(owner,lock));
  if(inputs.some(item=>["supply","supply-quality","supply-advanced"].includes(item.packageId))){
    sourceLocks.push(lock=>lockSupplyProcurementPackageObligationSources(owner,lock),lock=>lockSupplyInventoryPackageObligationSources(owner,lock));
    if(inputs.some(item=>item.packageId!=="supply-advanced"&&["supply","supply-quality"].includes(item.packageId)))sourceLocks.push(lock=>lockSupplyQualityPackageObligationSources(owner,lock));
    sourceLocks.push(lock=>lockSupplyFinancePackageObligationSources(owner,lock));
  }
  // Shared hospitality units acquire native Hotel then POS sources once,
  // before their included Finance/Stock owners, even in a mixed batch.
  if(inputs.some(item=>["pms","pos-hotel-charge","pms-minibar-linen"].includes(item.packageId)))sourceLocks.push(lock=>lockPmsPackageObligationSources(owner,lock));
  if(inputs.some(item=>["pos","pms","pos-restaurant","pos-hotel-charge"].includes(item.packageId)))sourceLocks.push(lock=>lockPosPackageObligationSources(owner,lock));
  if(inputs.some(item=>item.packageId==="pos"))sourceLocks.push(lock=>lockPosFinancePackageObligationSources(owner,lock),lock=>lockPosInventoryPackageObligationSources(owner,lock));
  if(inputs.some(item=>item.packageId==="pms"))sourceLocks.push(lock=>lockPmsFinancePackageObligationSources(owner,lock),lock=>lockPmsInventoryPackageObligationSources(owner,lock));
  if(inputs.some(item=>["pos-restaurant","pos-hotel-charge","pms-minibar-linen"].includes(item.packageId)))sourceLocks.push(lock=>lockHospitalityAddonFinancePackageObligationSources(owner,lock),lock=>lockHospitalityAddonInventoryPackageObligationSources(owner,lock));
  if (inputs.some(item => ["support","support-portal"].includes(item.packageId))) sourceLocks.push(lock => lockSupportPackageObligationSources(owner, lock));
  if (inputs.some(item => item.packageId === "finance" || item.packageId === "finance-advanced")) sourceLocks.push(lock => lockFinancePackageObligationSources(owner, lock));
  if(inputs.some(item=>item.packageId==="manufacturing"||item.packageId==="manufacturing-mrp"))sourceLocks.push(lock=>lockManufacturingPackageObligationSources(owner,lock),lock=>lockManufacturingPlanningPackageObligationSources(owner,lock),lock=>lockManufacturingQualityPackageObligationSources(owner,lock),lock=>lockManufacturingInventoryPackageObligationSources(owner,lock),lock=>lockManufacturingFinancePackageObligationSources(owner,lock));
  if(inputs.some(item=>item.packageId==="learn"))sourceLocks.push(lock=>lockLearningPackageObligationSources(owner,lock),lock=>lockLearningPeoplePackageObligationSources(owner,lock));
  if(inputs.some(item=>item.packageId==="recruit"))sourceLocks.push(lock=>lockTalentPackageObligationSources(owner,lock),lock=>lockTalentPeoplePackageObligationSources(owner,lock));
  if(inputs.some(item=>item.packageId==="legal"))sourceLocks.push(lock=>lockLegalPackageObligationSources(owner,lock));
  if(inputs.some(item=>["assets","assets-preventive","assets-parts-costs"].includes(item.packageId)))sourceLocks.push(lock=>lockAssetPackageObligationSources(owner,lock),lock=>lockMaintenancePackageObligationSources(owner,lock),lock=>lockAssetsFinancePackageObligationSources(owner,lock));
  if(inputs.some(item=>["projects","projects-costs"].includes(item.packageId)))sourceLocks.push(lock=>lockProjectPackageObligationSources(owner,lock),lock=>lockProjectPeoplePackageObligationSources(owner,lock),lock=>lockProjectFinancePackageObligationSources(owner,lock));
  if(inputs.some(item=>["people","people-workforce"].includes(item.packageId)))sourceLocks.push(lock=>lockPeoplePackageObligationSources(owner,lock),lock=>lockPeopleBenefitsPackageObligationSources(owner,lock),lock=>lockPeopleFinancePackageObligationSources(owner,lock));
  if(inputs.some(item=>["people-payroll","people-expenses"].includes(item.packageId)))sourceLocks.push(lock=>lockPeopleAddonPackageObligationSources(owner,lock),lock=>lockPeopleAddonFinancePackageObligationSources(owner,lock));
  if(inputs.some(item=>item.packageId==="people-performance"))sourceLocks.push(lock=>lockPerformancePackageObligationSources(owner,lock));
  if(inputs.some(item=>item.packageId==="people-benefits"))sourceLocks.push(lock=>lockBenefitPackageObligationSources(owner,lock),lock=>lockBenefitPeoplePackageObligationSources(owner,lock),lock=>lockBenefitFinancePackageObligationSources(owner,lock));
  if(inputs.some(item=>item.packageId==="sales-marketing"))sourceLocks.push(lock=>lockMarketingPackageObligationSources(owner,lock),lock=>lockMarketingCRMPackageObligationSources(owner,lock),lock=>lockMarketingFinancePackageObligationSources(owner,lock));
  if(inputs.some(item=>item.packageId==="sales-portal"))sourceLocks.push(lock=>lockPortalAuthorityPackageObligationSources(owner,lock),lock=>lockPortalCustomerPackageObligationSources(owner,lock));
  if(inputs.some(item=>item.packageId==="sales-ecommerce"))sourceLocks.push(lock=>lockEcommercePackageObligationSources(owner,lock),lock=>lockEcommerceSalesPackageObligationSources(owner,lock),lock=>lockEcommerceInventoryPackageObligationSources(owner,lock),lock=>lockEcommerceFinancePackageObligationSources(owner,lock));
  if(inputs.some(item=>item.packageId==="sales"))sourceLocks.push(lock=>lockSalesPackageObligationSources(owner,lock),lock=>lockSalesCRMPackageObligationSources(owner,lock),lock=>lockSalesInventoryPackageObligationSources(owner,lock),lock=>lockSalesFinancePackageObligationSources(owner,lock));
  if(inputs.some(item=>["service","service-parts-assets"].includes(item.packageId)))sourceLocks.push(lock=>lockServicePackageObligationSources(owner,lock),lock=>lockServiceSupportPackageObligationSources(owner,lock),lock=>lockServiceInventoryPackageObligationSources(owner,lock),lock=>lockServiceAssetPackageObligationSources(owner,lock),lock=>lockServiceFinancePackageObligationSources(owner,lock));
  return withPackageSettlementSources(owner, companyId, sourceLocks, async transaction => {
    const before = await readCompanyPackageGrants(transaction, companyId);
    const result = await run(transaction);
    // Own writes and deferred owner projections are not excluded by source
    // SHARE locks. Flush them, then inspect the final image while all fences
    // remain held. A committed receipt replay does not repeat a withdrawal.
    await flushFinanceDeferredEffects(transaction);
    const after = await readCompanyPackageGrants(transaction, companyId);
    for (const item of inputs) {
      const previous = before.find(grant => grant.packageId === item.packageId) ?? null, current = after.find(grant => grant.packageId === item.packageId);
      if (current && current.version !== previous?.version && requiresPackageSettlementReview(previous, current)) {
        const observation = await readOwnerPackageObligations(owner, transaction, { ...item, packageVersion: previous?.packageVersion ?? current.packageVersion, now: Date.now() });
        if (observation.total !== 0) throw new Error("PACKAGE_OBLIGATIONS_REMAIN");
      }
    }
    if (finalize) await finalize(transaction);
    return result;
  });
}

/** Only concrete owner readers are registered. Missing owners remain errors. */
export function readOwnerPackageObligations(owner: PostgresDatabase, transaction: SqlDatabase, input: PackageObligationInput) {
  owner.assertActiveTransaction(transaction);
  return readPackageObligations(transaction, input, {
    ...(["pos-restaurant","pos-hotel-charge","pms-minibar-linen"].includes(input.packageId)&&input.packageVersion===1?{
      ...(input.packageId!=="pms-minibar-linen"?{pos:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>posAddonPackageObligations(owner,database,request)}:{}),
      ...(input.packageId!=="pos-restaurant"?{pms:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>pmsAddonPackageObligations(owner,database,request)}:{}),
      finance:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>hospitalityAddonFinancePackageObligations(owner,database,request),
      inventory:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>hospitalityAddonInventoryPackageObligations(owner,database,request),
    }:{}),
    ...(["supply","supply-quality","supply-advanced"].includes(input.packageId)&&input.packageVersion===1?{
      ...(input.packageId==="supply"?{procurement:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>supplyProcurementPackageObligations(owner,database,request)}:{}),
      inventory:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>supplyInventoryPackageObligations(owner,database,request),
      ...(input.packageId!=="supply-advanced"?{quality:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>supplyQualityPackageObligations(owner,database,request)}:{}),
      finance:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>supplyFinancePackageObligations(owner,database,request),
    }:{}),
    ...(["work","work-social","work-messages","work-routines"].includes(input.packageId)&&input.packageVersion===1?{
      collaboration:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>workCollaborationPackageObligations(owner,database,request),
      ...(input.packageId!=="work-messages"?{operations:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>workOperationsPackageObligations(owner,database,request)}:{}),
    }:{}),
    ...(input.packageId==="sales-portal"&&input.packageVersion===1?{
      customer_authority:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>portalAuthorityPackageObligations(owner,database,request),
      customer_360:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>portalCustomerPackageObligations(owner,database,request),
    }:{}),
    ...(input.packageId==="sales-ecommerce"&&input.packageVersion===1?{
      ecommerce:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>ecommercePackageObligations(owner,database,request),
      sales:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>ecommerceSalesPackageObligations(owner,database,request),
      inventory:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>ecommerceInventoryPackageObligations(owner,database,request),
      finance:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>ecommerceFinancePackageObligations(owner,database,request),
    }:{}),
    ...(input.packageId==="sales"&&input.packageVersion===1?{
      sales:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>salesPackageObligations(owner,database,request),
      crm:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>salesCRMPackageObligations(owner,database,request),
      inventory:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>salesInventoryPackageObligations(owner,database,request),
      finance:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>salesFinancePackageObligations(owner,database,request),
    }:{}),
    ...(["service","service-parts-assets"].includes(input.packageId)&&input.packageVersion===1?{
      field_service:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>servicePackageObligations(owner,database,request),
      support:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>serviceSupportPackageObligations(owner,database,request),
      inventory:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>serviceInventoryPackageObligations(owner,database,request),
      assets:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>serviceAssetPackageObligations(owner,database,request),
      finance:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>serviceFinancePackageObligations(owner,database,request),
    }:{}),
    ...(!["service","service-parts-assets"].includes(input.packageId)?{support: (database:SqlDatabase, request:PackageObligationInput & {moduleId:string}) => supportPackageObligations(owner, database, request)}:{}),
    ...(["people-payroll","people-expenses"].includes(input.packageId)&&input.packageVersion===1?{
      people_payroll:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>peopleAddonPackageObligations(owner,database,request),
      finance:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>peopleAddonFinancePackageObligations(owner,database,request),
    }:{}),
    ...(input.packageId==="people-performance"&&input.packageVersion===1?{people_payroll:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>performancePackageObligations(owner,database,request)}:{}),
    ...(input.packageId==="people-benefits"&&input.packageVersion===1?{
      benefits:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>benefitPackageObligations(owner,database,request),
      people_payroll:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>benefitPeoplePackageObligations(owner,database,request),
      finance:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>benefitFinancePackageObligations(owner,database,request),
    }:{}),
    ...(input.packageId==="sales-marketing"&&input.packageVersion===1?{
      marketing:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>marketingPackageObligations(owner,database,request),
      crm:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>marketingCRMPackageObligations(owner,database,request),
      finance:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>marketingFinancePackageObligations(owner,database,request),
    }:{}),
    ...(["people","people-workforce"].includes(input.packageId)&&input.packageVersion===1?{
      people_payroll:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>peoplePackageObligations(owner,database,request),
      benefits:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>peopleBenefitsPackageObligations(owner,database,request),
      finance:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>peopleFinancePackageObligations(owner,database,request),
    }:{}),
    ...(["projects","projects-costs"].includes(input.packageId)&&input.packageVersion===1?{
      projects:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>projectPackageObligations(owner,database,request),
      people_payroll:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>projectPeoplePackageObligations(owner,database,request),
      finance:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>projectFinancePackageObligations(owner,database,request),
    }:{}),
    ...(input.packageId==="learn"&&input.packageVersion===1?{
      learning:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>learningPackageObligations(owner,database,request),
      people_payroll:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>learningPeoplePackageObligations(owner,database,request),
    }:{}),
    ...(input.packageId === "finance" || input.packageId === "finance-advanced" ? {
      finance: (database: SqlDatabase, request: PackageObligationInput & { moduleId: string }) => financePackageObligations(owner, database, request),
    } : {}),
    ...(["manufacturing","manufacturing-mrp"].includes(input.packageId)&&input.packageVersion===1?{
      manufacturing:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>manufacturingPackageObligations(owner,database,request),
      planning:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>manufacturingPlanningPackageObligations(owner,database,request),
      quality:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>manufacturingQualityPackageObligations(owner,database,request),
      inventory:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>manufacturingInventoryPackageObligations(owner,database,request),
      finance:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>manufacturingFinancePackageObligations(owner,database,request),
    }:{}),
    ...(input.packageId==="recruit"&&input.packageVersion===1?{
      talent:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>talentPackageObligations(owner,database,request),
      people_payroll:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>talentPeoplePackageObligations(owner,database,request),
    }:{}),
    ...(input.packageId==="legal"&&input.packageVersion===1?{legal:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>legalPackageObligations(owner,database,request)}:{}),
    ...(["assets","assets-preventive","assets-parts-costs"].includes(input.packageId)&&input.packageVersion===1?{
      assets:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>assetPackageObligations(owner,database,request),
      maintenance:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>maintenancePackageObligations(owner,database,request),
      finance:(database:SqlDatabase,request:PackageObligationInput & {moduleId:string})=>assetsFinancePackageObligations(owner,database,request),
    }:{}),
    // Included POS handoffs do not certify the full Finance/Stock products or
    // another product's foundation. Register them only for the exact package.
    ...(input.packageId === "pos" && input.packageVersion === 1 ? {
      pos: (database: SqlDatabase, request: PackageObligationInput & { moduleId: string }) => posPackageObligations(owner, database, request),
      finance: (database: SqlDatabase, request: PackageObligationInput & { moduleId: string }) => posFinancePackageObligations(owner, database, request),
      inventory: (database: SqlDatabase, request: PackageObligationInput & { moduleId: string }) => posInventoryPackageObligations(owner, database, request),
    } : {}),
    ...(input.packageId === "pms" && input.packageVersion === 1 ? {
      pms: (database: SqlDatabase, request: PackageObligationInput & { moduleId: string }) => pmsPackageObligations(owner, database, request),
      pos: (database: SqlDatabase, request: PackageObligationInput & { moduleId: string }) => pmsPosPackageObligations(owner, database, request),
      finance: (database: SqlDatabase, request: PackageObligationInput & { moduleId: string }) => pmsFinancePackageObligations(owner, database, request),
      inventory: (database: SqlDatabase, request: PackageObligationInput & { moduleId: string }) => pmsInventoryPackageObligations(owner, database, request),
    } : {}),
  });
}

/** Normal Commercial lifecycle port, using the persisted version in this unit. */
export async function ownerPackageObligations(owner: PostgresDatabase, transaction: SqlDatabase, companyId: string, packageId: string) {
  owner.assertActiveTransaction(transaction);
  const current = (await readCompanyPackageGrants(transaction, companyId)).find(grant => grant.packageId === packageId);
  if (!current) throw new Error("PACKAGE_GRANT_REQUIRED");
  return (await readOwnerPackageObligations(owner, transaction, {
    companyId, packageId, packageVersion: current.packageVersion, now: Date.now(),
  })).total;
}
