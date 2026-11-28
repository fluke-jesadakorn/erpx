import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import type { LegacySource, Scope } from '@erpx/files-server/domain/attachments/migration';
import { legacyAttachmentScopesupport } from '@erpx/support-server/public/attachment-migration-owner';
import { legacyAttachmentScoperecords, type HoldingIdentity } from '@erpx/records-server/public/attachment-migration-owner';
import { legacyAttachmentScopepeoplepayroll, legacyExpensePaymentScope } from '@erpx/people-payroll-server/public/attachment-migration-owner';
import { legacyAttachmentScopefinance } from '@erpx/finance-server/public/attachment-migration-owner';
import { legacyAttachmentScopeoperations } from '@erpx/operations-server/public/attachment-migration-owner';
import { legacyAttachmentScopecollaboration } from '@erpx/collaboration-server/public/attachment-migration-owner';
import { legacyAttachmentScopequality } from '@erpx/quality-server/public/attachment-migration-owner';
import { legacyAttachmentScopecommerce } from '@erpx/commerce-server/public/attachment-migration-owner';
import { legacyBranchBrandScope, legacyCompanyBrandScope, legacyHoldingBrandCompanies } from '@erpx/organization-server/public/attachment-migration-owner';

export { legacyHoldingBrandCompanies };

type ScopeRow = {
  company_id: string;
  branch_id: string | null;
  department_id: string | null;
  version: number;
};

const readers: Record<string, (db: SqlDatabase, kind: string, id: string) => Promise<ScopeRow | null>> = {
  support_ticket: legacyAttachmentScopesupport,
  leave_request: legacyAttachmentScopepeoplepayroll,
  expense_claim: legacyAttachmentScopepeoplepayroll,
  invoice_payment: legacyAttachmentScopefinance,
  staff_task: legacyAttachmentScopeoperations,
  social_draft: legacyAttachmentScopecollaboration,
  quality_hold: legacyAttachmentScopequality,
  commerce_order_delivery: legacyAttachmentScopecommerce,
};

const project = (row: ScopeRow | null): Scope | null =>
  row
    ? {
        companyId: row.company_id,
        branchId: row.branch_id,
        departmentId: row.department_id,
        version: Number(row.version),
      }
    : null;

/**
 * The `employee_request` reader is owner-native (lane 35). It needs the caller's
 * WorkContext to select exactly one registered Holding; the other readers stay
 * owner-local and are not given a Holding identity.
 */
export async function legacyAttachmentOwnerScope(
  db: SqlDatabase,
  source: LegacySource,
  context?: HoldingIdentity,
): Promise<Scope | null> {
  if (source.owner_type === 'expense_payment') {
    return project(await legacyExpensePaymentScope(db, source.owner_id));
  }
  if (source.owner_type === 'branch_brand') {
    const row = await legacyBranchBrandScope(db, source.owner_id);
    return row
      ? { companyId: row.company_id, branchId: row.branch_id, departmentId: null, version: 1 }
      : null;
  }
  if (source.owner_type === 'company_storefront_brand') {
    const row = await legacyCompanyBrandScope(db, source.owner_id);
    return row
      ? { companyId: row.company_id, branchId: null, departmentId: null, version: 1 }
      : null;
  }
  if (source.owner_type === 'employee_request') {
    return project(
      await legacyAttachmentScoperecords(db, source.owner_type, source.owner_id, context),
    );
  }
  return project(
    await (readers[source.owner_type] ?? (() => Promise.resolve(null)))(
      db,
      source.owner_type,
      source.owner_id,
    ),
  );
}
