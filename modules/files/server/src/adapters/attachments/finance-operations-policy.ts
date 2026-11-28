import {authorizeFinanceAttachmentPurchase} from "@erpx/finance-server/public/attachment-purchases";
import "server-only";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import type { SqlDatabase, SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import type { AttachmentOperation, AttachmentPolicy } from "@erpx/files-server/public/attachments/types";
import { createFinanceAttachmentPolicy, type FinanceAttachmentUnit } from "@erpx/files-server/application/attachments/finance-operations-policy";
import {getOvertime} from '@erpx/people-payroll-server/public/overtime-read';
import { FinanceAttachmentFinanceFacts } from "@erpx/finance-server/public/attachments-finance-facts";
import { FinanceAttachmentPosFacts } from "@erpx/pos-server/public/attachments-finance-facts";
import { FinanceAttachmentPmsFacts } from "@erpx/pms-server/public/attachments-finance-facts";
import { FinanceAttachmentPeoplePayrollFacts } from "@erpx/people-payroll-server/public/attachments-finance-facts";
import { FinanceAttachmentOrganizationFacts } from "@erpx/organization-server/public/attachments-finance-facts";
import { FinanceAttachmentIdentityAccessFacts } from "@erpx/identity-access-server/public/attachments-finance-facts";
import { FinanceAttachmentPlatformAdministrationFacts } from "@erpx/platform-administration-server/public/attachments-finance-facts";
import { FinanceAttachmentManageErpxFacts } from "@erpx/manage-erpx-server/public/attachments-finance-facts";

function unit(db: SqlDatabase): FinanceAttachmentUnit<SqlStatement> {
  return {
    readOwner1: facts => FinanceAttachmentFinanceFacts(db).readOwner1(facts),
    readOwner2: facts => FinanceAttachmentPosFacts(db).readOwner2(facts),
    readOwner3: facts => FinanceAttachmentPosFacts(db).readOwner3(facts),
    readOwner4: facts => FinanceAttachmentPmsFacts(db).readOwner4(facts),
    readOwner5: facts => FinanceAttachmentPmsFacts(db).readOwner5(facts),
    readOwner6: facts => FinanceAttachmentPeoplePayrollFacts(db).readOwner6(facts),
    readOwner7: facts => FinanceAttachmentOrganizationFacts(db).readOwner7(facts),
    readOwner8: facts => FinanceAttachmentOrganizationFacts(db).readOwner8(facts),
    readOwner9: facts => FinanceAttachmentIdentityAccessFacts(db).readOwner9(facts),
    readOwner10: facts => FinanceAttachmentPlatformAdministrationFacts(db).readOwner10(facts),
    readOwner11: facts => FinanceAttachmentPeoplePayrollFacts(db).readOwner11(facts),
    readOwner12: facts => FinanceAttachmentManageErpxFacts(db).readOwner12(facts),
    overtime: (actor, id) => getOvertime(actor, id, db),
    versionGuard: (...args) => {
      const a = args as Parameters<FinanceAttachmentUnit<SqlStatement>["versionGuard"]>;
      const base = a.slice(0, 6) as Parameters<ReturnType<typeof FinanceAttachmentPosFacts>["versionGuard"]>;
      return FinanceAttachmentFinanceFacts(db).versionGuard(...a)
        ?? FinanceAttachmentPosFacts(db).versionGuard(...base)
        ?? FinanceAttachmentPmsFacts(db).versionGuard(...base)
        ?? FinanceAttachmentPeoplePayrollFacts(db).versionGuard(...base)
        ?? FinanceAttachmentOrganizationFacts(db).versionGuard(...base)
        ?? FinanceAttachmentIdentityAccessFacts(db).versionGuard(...base)
        ?? FinanceAttachmentPlatformAdministrationFacts(db).versionGuard(...base)
        ?? FinanceAttachmentManageErpxFacts(db).versionGuard(...base);
    },
    versionStep: (...args) => {
      const a = args as Parameters<FinanceAttachmentUnit<SqlStatement>["versionStep"]>;
      // Only the Finance owner consumes the authorized actor context. The
      // remaining owner facts keep their original four-argument signature.
      const base = a.slice(0, 4) as Parameters<ReturnType<typeof FinanceAttachmentPosFacts>["versionStep"]>;
      return (FinanceAttachmentFinanceFacts(db).versionStep(...a)
        ?? FinanceAttachmentPosFacts(db).versionStep(...base)
        ?? FinanceAttachmentPmsFacts(db).versionStep(...base)
        ?? FinanceAttachmentPeoplePayrollFacts(db).versionStep(...base)
        ?? FinanceAttachmentOrganizationFacts(db).versionStep(...base)
        ?? FinanceAttachmentIdentityAccessFacts(db).versionStep(...base)
        ?? FinanceAttachmentPlatformAdministrationFacts(db).versionStep(...base)
        ?? FinanceAttachmentManageErpxFacts(db).versionStep(...base)) as SqlStatement | null;
    },
  };
}
const service = createFinanceAttachmentPolicy<SqlStatement>({ clock: { now: () => Date.now() }, error: (status, code, message) => new ApiError(status, code, message) });
export const financeOperationsAttachmentPolicies = service.financeOperationsAttachmentPolicies, forbiddenAttachmentPurposeKeys = service.forbiddenAttachmentPurposeKeys;
export async function authorizeFinanceOperationsAttachmentTarget(db: SqlDatabase, actor: WorkContext, policy: AttachmentPolicy, targetId: string, operation: AttachmentOperation) { const scope=await service.authorizeFinanceOperationsAttachmentTarget(unit(db),actor,policy,targetId,operation);if(scope)await authorizeFinanceAttachmentPurchase(db,actor,policy.targetType,targetId,operation);return scope; }
export function financeOperationsAttachmentTargetVersionGuard(db: SqlDatabase, commandId: string, targetType: string, targetId: string, companyId: string, version: number, index: number, actor?: WorkContext) { return service.financeOperationsAttachmentTargetVersionGuard(unit(db), commandId, targetType, targetId, companyId, version, index, actor); }
export function financeOperationsAttachmentRecordVersionStep(db: SqlDatabase, targetType: string, targetId: string, expectedVersion: number, now: number, context?: { actor: WorkContext; commandId: string }) { return service.financeOperationsAttachmentRecordVersionStep(unit(db), targetType, targetId, expectedVersion, now, context); }
export function requiresSanitizedEvidenceAttestation(targetType: string) { return service.requiresSanitizedEvidenceAttestation(targetType); }
export function assertSanitizedEvidenceMetadata(policy: AttachmentPolicy, filename: string, caption: string | null) { return service.assertSanitizedEvidenceMetadata(policy, filename, caption); }
