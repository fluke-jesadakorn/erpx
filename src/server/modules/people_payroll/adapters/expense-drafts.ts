import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as filesProjectionSources } from "../../files/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as financeProjectionSources } from "../../finance/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as identity_accessProjectionSources } from "../../identity_access/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as operationsProjectionSources } from "../../operations/public/hospitality-owner-projection-sources";
import { createExpenseRequirements } from "../application/expense-evidence";
import { createSaveExpenseDraft } from "../application/expense-commands";
import { expenseCommandPort, expenseMutation } from "./expense-command-port";
/** Native Expense adapter; preserves legacy persistence and command semantics. */
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { assertion } from "@/server/modules/workflow/public/commands";
import { ApiError } from "@/server/modules/runtime/public/http";
import { activeStaff } from "@/server/modules/operations/public/staff-policy";
import { expenseAuthoritySnapshot } from "./expense-routing";
import { canManagePeople } from "@/server/modules/people_payroll/public/people-directory";
import { decimalToMinor } from "@/server/modules/finance/public/finance";
import { expenseFromDatabase, type ExpenseRow } from "./expenses";
import { attachmentsV2Readable } from "@/server/modules/files/public/attachments/feature";

const optionalText = (max: number) => z.string().trim().max(max).optional();
const optionalMoney = z
  .union([z.literal(""), z.string().regex(/^\d+(\.\d{1,2})?$/)])
  .optional();
export const expenseDraftSchema = z.object({
  payeePartyId: optionalText(160),
  requestKind: z.enum(["expense","payment"]).optional(),
  personId: optionalText(160),
  expenseDate: z.union([z.literal(""), z.iso.date()]).optional(),
  merchant: optionalText(200),
  receiptReference: optionalText(100),
  businessPurpose: optionalText(2000),
  currency: z.union([z.literal(""), z.string().regex(/^[A-Z]{3}$/)]).optional(),
  amount: optionalMoney,
  tax: optionalMoney,
  categoryAccountId: optionalText(160),
});
export const expenseUpdateSchema = expenseDraftSchema.extend({
  expectedVersion: z.number().int().positive(),
});

export function nativeExpenseRequirements(db: SqlDatabase) {
  return createExpenseRequirements({
    async employment(row) {
      try {
        const staff = await activeStaff(db, String(row.company_id), String(row.claimant_user_id));
        const level = staff.level_id && await db.prepare(`SELECT 1 FROM (${operationsProjectionSources.staff_levels}) staff_levels WHERE id=? AND company_id=? AND active=1`).bind(staff.level_id, row.company_id).first();
        return { startDate: staff.start_date, endDate: staff.end_date, activeLevel: Boolean(level) };
      } catch (error) {
        if (!(error instanceof ApiError)) throw error;
        return { unavailable: error.message };
      }
    },
    async hasReceipt(row) {
      return Boolean(await db.prepare(attachmentsV2Readable()
        ? `SELECT 1 FROM (${filesProjectionSources.attachments}) a JOIN (${filesProjectionSources.attachment_links}) l ON l.attachment_id=a.id
             JOIN (${filesProjectionSources.attachment_sets}) s ON s.id=l.set_id
             WHERE a.owner_type='expense_claim' AND a.owner_id=? AND a.revoked_at IS NULL
               AND a.source_state='available' AND l.state='bound'
               AND s.target_type='expense_claim' AND s.target_id=a.owner_id LIMIT 1`
        : `SELECT 1 FROM (${filesProjectionSources.attachments}) attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL LIMIT 1`).bind(row.id).first());
    },
  });
}
export async function expenseRequirements(db: SqlDatabase, row: ExpenseRow) {
  return nativeExpenseRequirements(db)(row);
}

export function nativeExpenseDrafts(write: SqlDatabase) {
  return createSaveExpenseDraft({
    ...expenseCommandPort(write),
    expense: (actor, id) => expenseFromDatabase(write, actor, id),
    authority: actor => expenseAuthoritySnapshot(write, actor.companyId),
    claimant: (actor, id) => write.prepare("SELECT id,user_id,legal_name FROM hr_people WHERE id=? AND company_id=? AND merged_into_id IS NULL").bind(id, actor.companyId).first<{ id: string; user_id: string | null; legal_name: string }>(),
    async staff(actor, claimant) {
      try { return await activeStaff(write, actor.companyId, claimant); }
      catch (error) { if (error instanceof ApiError) return null; throw error; }
    },
    latestEngagement: (actor, personId) => write.prepare("SELECT branch_id,department_id FROM hr_engagements WHERE person_id=? AND company_id=? ORDER BY start_date DESC LIMIT 1").bind(personId, actor.companyId).first<{ branch_id: string | null; department_id: string | null }>(),
    canManagePeople,
    money: decimalToMinor,
    async prepareDraft(actor, key, input, facts, capturedAuthority) {
      const { recordId, current, claimant, claimantName, personId, staff, amount, tax, category, currency, requestKind, payeePartyId } = facts;
      const authority = capturedAuthority as Awaited<ReturnType<typeof expenseAuthoritySnapshot>>;
      const now = Date.now(), version = current ? input.expectedVersion + 1 : 1;
      const steps = [
    authority.guard(key),
    assertion(
      write,
      key,
      "claimant-access",
      `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE user_id=? AND company_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>?)`,
      [claimant, actor.companyId, now, now],
    ),
  ];

  if(payeePartyId)steps.push(assertion(write,key,"supplier-payee",`EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND kind='supplier' AND status='active' AND currency=?)`,[payeePartyId,actor.companyId,currency]));
  if (category)
    steps.push(
      assertion(
        write,
        key,
        "category",
        `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_accounts}) a JOIN (${commerceProjectionSources.commerce_settings}) s ON s.chart_id=a.chart_id AND s.company_id=a.company_id WHERE a.id=? AND a.company_id=? AND a.category='expense' AND a.status='active' AND (?::text IS NULL OR (a.currency=? AND s.currency=?)))`,
        [category, actor.companyId, currency, currency, currency],
      ),
    );
  if (current)
    steps.push(
      assertion(
        write,
        key,
        "expense-version",
        "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN ('draft','rejected'))",
        [recordId, actor.companyId, input.expectedVersion],
      ),
    );
  const values = [
    requestKind,
    payeePartyId || null,
    personId,
    staff?.engagement_id ?? null,
    staff?.branch_id ?? null,
    staff?.department_id ?? null,
    claimant,
    claimantName,
    input.expenseDate || null,
    input.merchant || null,
    currency,
    amount,
    tax,
    category,
    input.receiptReference || null,
    input.businessPurpose ?? "",
    current?.prepared_by ?? actor.userId,
    now,
  ];
  steps.push(
    current
      ? write
          .prepare(
            "UPDATE finance_expenses SET request_kind=?,payee_party_id=?,employee_id=?,engagement_id=?,branch_id=?,department_id=?,claimant_user_id=?,claimant_name=?,expense_date=?,merchant=?,currency=?,amount_minor=?,tax_minor=?,category_account_id=?,receipt_reference=?,business_purpose=?,prepared_by=?,updated_at=?,status='draft',workflow_version=2,workflow_stage='draft',version=version+1 WHERE id=?",
          )
          .bind(...values, recordId)
      : write
          .prepare(
            "INSERT INTO finance_expenses(request_kind,payee_party_id,employee_id,engagement_id,branch_id,department_id,claimant_user_id,claimant_name,expense_date,merchant,currency,amount_minor,tax_minor,category_account_id,receipt_reference,business_purpose,prepared_by,updated_at,id,company_id,created_by,created_at,workflow_version) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,2)",
          )
          .bind(...values, recordId, actor.companyId, actor.userId, now),
  );
  
      return expenseMutation({ id: recordId, status: "draft", version }, steps);
    },
  });
}
export async function saveExpenseDraft(actor: WorkContext, id: string | null, input: z.infer<typeof expenseUpdateSchema>, key: string) {
  const { write } = await holdingResources(actor.holdingId);
  return nativeExpenseDrafts(write)(actor, id, input, key);
}
