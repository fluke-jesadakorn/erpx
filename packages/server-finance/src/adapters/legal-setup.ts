import "server-only";
import { readPurchasedAccesses } from "@erpx/server-commercial/public/purchase-command-boundary";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { assignmentReadSql } from "@erpx/server-identity_access/public/record-source-authority";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import { financeReferenceCapabilities, financePurchaseControls, requireFinanceCurrentAuthority } from "@erpx/server-finance/adapters/purchase-boundary";
import { taxIdentityWorkspace } from "@erpx/server-finance/adapters/finance-tax-identities";
import { withholdingWorkspace } from "@erpx/server-finance/adapters/finance-withholding";
import { withholdingMonthlyWorkspace } from "@erpx/server-finance/adapters/finance-withholding-remittance";
import {packageClockSql,purchasedCapabilityPredicate} from "@erpx/server-commercial/public/purchased-capability-sql";
import {scheduledFeaturePredicate} from "@erpx/server-pms/public/hospitality-rollout";
import {ApiError} from "@erpx/server-runtime/public/http";
import type {FinanceProductPresentationOptions} from "@erpx/server-finance/adapters/record-presentation";
import type {FinancePurchaseControls} from "@erpx/server-finance/domain/purchase-controls";

type LegalSetupScope = Pick<WorkContext, "holdingId" | "companyId" | "branchId" | "departmentId" | "userId" | "assignmentId" | "role"> & { assignmentVersion: number };

/** The included foundation exposes legal setup, never the full Finance product.
 * The normal role service still owns selection, release and session checks. */
export async function financeProductLegalSetupEntry(scope: LegalSetupScope) {
  if (scope.branchId || scope.departmentId || !Number.isSafeInteger(scope.assignmentVersion) || scope.assignmentVersion<1) return null;
  const database = (await holdingResources(scope.holdingId)).write;
  const access = await readPurchasedAccesses(database, "enforced", scope.companyId, financeReferenceCapabilities());
  const permitted = Object.entries(access).filter(([,value])=>value.history);if(!permitted.length)return null;
  const current = await database.prepare(`WITH legal_setup_clock AS MATERIALIZED(SELECT ${packageClockSql} now_ms)
    SELECT 1 FROM (SELECT ?::text company_id,?::text branch_id,?::text department_id) legal_setup_scope CROSS JOIN legal_setup_clock
    WHERE ${assignmentReadSql("legal_setup_scope", "finance.read")} AND EXISTS(SELECT 1 FROM assignments WHERE id=? AND version=?)
    AND ${scheduledFeaturePredicate("legal_setup_scope.company_id","finance")}
    AND (${permitted.map(([capability])=>purchasedCapabilityPredicate("legal_setup_scope.company_id",capability,"history","legal_setup_clock.now_ms")).join(" OR ")})`)
    .bind(scope.companyId,scope.branchId,scope.departmentId,scope.assignmentId,scope.userId,scope.role,scope.assignmentId,scope.assignmentVersion).first();
  return current ? { revision: permitted[0][1].revision } : null;
}

async function strictLegalSetupScope(actor:WorkContext,captured?:LegalSetupScope):Promise<LegalSetupScope>{
 const database=(await holdingResources(actor.holdingId)).write;await requireFinanceCurrentAuthority(database,actor,`finance-legal-product:${crypto.randomUUID()}`);
 const version=captured?.assignmentVersion??actor.authority?.assignmentVersion??actor.iam?.queuedAssignmentVersion??await database.prepare("SELECT version FROM assignments WHERE id=? AND user_id=? AND company_id=? AND role=?").bind(actor.assignmentId,actor.userId,actor.companyId,actor.role).first<number>("version");
 const scope=captured??{...actor,assignmentVersion:version??0};if(!await financeProductLegalSetupEntry(scope))throw new ApiError(403,"PACKAGE_REQUIRED","An authorized accounting product or included foundation is required for Company legal setup.");return scope;
}

/** This page must remain usable by included posting foundations. Do not read
 * supplier withholding, bank or monthly tax data merely to show legal setup. */
export async function financeLegalSetupWorkspace(actor: WorkContext,options:FinanceProductPresentationOptions={}) {
  const configurationCapabilities = financeReferenceCapabilities();
  const strictScope=options.strictProduct?await strictLegalSetupScope(actor):null;
  const strictPurchases:FinancePurchaseControls|undefined=strictScope?Object.fromEntries(Object.entries(await readPurchasedAccesses((await holdingResources(actor.holdingId)).write,"enforced",actor.companyId,[...new Set([...configurationCapabilities,"finance.tax"])])).map(([capability,value])=>[capability,{newWork:value.newWork,settleExisting:value.settleExisting,history:value.history,revision:value.revision}])):undefined;
  const [identities, purchases] = await Promise.all([
    taxIdentityWorkspace(actor),
    strictPurchases??financePurchaseControls(actor, [...new Set([...configurationCapabilities, "finance.tax"])]),
  ]);
  const sourceOnly = purchases !== undefined && !purchases["finance.accounting"].history;
  const tax = !purchases || purchases["finance.tax"].history
    ? await Promise.all([withholdingWorkspace(actor), withholdingMonthlyWorkspace(actor)]).then(([withholding, monthly]) => ({ withholding, monthly }))
    : null;
  // Omit an unavailable GL control from the included-only provider. It should
  // describe legal setup, rather than advise the user to purchase general GL.
  const identityPurchases = sourceOnly && purchases
    ? Object.fromEntries(Object.entries(purchases).filter(([capability]) => capability !== "finance.accounting"))
    : purchases;
  if(strictScope)await strictLegalSetupScope(actor,strictScope);
  return { identities, tax, sourceOnly, purchases: identityPurchases, configurationCapabilities };
}
