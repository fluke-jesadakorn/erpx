import{nativeExpenseEmployment,nativeExpenseRequirements,expenseRequirements}from '@erpx/server-people_payroll/adapters/expense-requirements';
export{nativeExpenseEmployment,nativeExpenseRequirements,expenseRequirements};

import {withExpensePurchasedCommand} from "@erpx/server-people_payroll/adapters/expense-purchases";
import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/hospitality-owner-projection-sources";

import { hospitalityOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/hospitality-owner-projection-sources";

import { createSaveExpenseDraft } from "@erpx/server-people_payroll/application/expense-commands";
import { expenseCommandPort, expenseMutation } from "@erpx/server-people_payroll/adapters/expense-command-port";
/** Native Expense adapter; preserves legacy persistence and command semantics. */
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { ApiError } from "@erpx/server-runtime/public/http";
import { activeStaff } from "@erpx/server-operations/public/staff-policy";
import { expenseAuthoritySnapshot } from "@erpx/server-people_payroll/adapters/expense-routing";
import { canManagePeople } from "@erpx/server-people_payroll/public/people-directory";
import {decimalToMinor} from '@erpx/server-finance/public/money-primitives';
import {expenseFromDatabase} from '@erpx/server-people_payroll/adapters/expense-read';

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

function originalNativeExpenseDrafts(write: SqlDatabase) {
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
  return withExpensePurchasedCommand(write, actor, {key,purpose:"new-work"}, tx => nativeExpenseDrafts(tx)(actor, id, input, key));
}

export function nativeExpenseDrafts(database: SqlDatabase){return (...args:Parameters<ReturnType<typeof originalNativeExpenseDrafts>>)=>withExpensePurchasedCommand(database,args[0],{key:args[3],purpose:"new-work"},transaction=>originalNativeExpenseDrafts(transaction)(...args));}
