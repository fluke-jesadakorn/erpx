import {ppRd36TwoPredTail} from './pp-read-tail-r36-sources';
import { ppRd51AccountingCategoryGuard } from './pp-read-tail-r51-sources';
import { PP_RD23_FINANCE_EXPENSE_DRAFT_REJECTED_GUARD } from './pp-read-tail-r23-sources';
import { ppRd29FinancePartySupplierSql } from './pp-read-tail-r29-sources';
import { expenseDrafts } from '@erpx/collaboration-server/adapters/native-expense-drafts';
import{nativeExpenseEmployment,nativeExpenseRequirements,expenseRequirements}from '@erpx/people-payroll-server/adapters/expense-requirements';
export{nativeExpenseEmployment,nativeExpenseRequirements,expenseRequirements};

import {withExpensePurchasedCommand} from "@erpx/people-payroll-server/adapters/expense-purchases";

import { hospitalityOwnerProjectionSources as financeProjectionSources } from "@erpx/finance-server/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/identity-access-server/public/hospitality-owner-projection-sources";

import { createSaveExpenseDraft } from "@erpx/people-payroll-server/application/expense-commands";
import { expenseCommandPort, expenseMutation } from "@erpx/people-payroll-server/adapters/expense-command-port";
/** Native Expense adapter; preserves legacy persistence and command semantics. */
import type { SqlDatabase, SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import {assertion} from "@erpx/workflow-server/public/command-assertions";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { activeStaff } from "@erpx/operations-server/public/staff-policy";
import { expenseAuthoritySnapshot } from "@erpx/people-payroll-server/adapters/expense-routing";
import { canManagePeople } from "@erpx/people-payroll-server/public/people-directory";
import {decimalToMinor} from '@erpx/finance-server/public/money-primitives';
import {expenseFromDatabase} from '@erpx/people-payroll-server/adapters/expense-read';

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
    claimant: (actor, id) => write.prepare(ppRd36TwoPredTail("id,user_id,legal_name", "hr_people", "id", "company_id", " AND merged_into_id IS NULL")).bind(id, actor.companyId).first<{ id: string; user_id: string | null; legal_name: string }>(),
    async staff(actor, claimant) {
      try { return await activeStaff(write, actor.companyId, claimant); }
      catch (error) { if (error instanceof ApiError) return null; throw error; }
    },
    latestEngagement: (actor, personId) => write.prepare(ppRd36TwoPredTail("branch_id,department_id","hr_engagements","person_id","company_id"," ORDER BY start_date DESC LIMIT 1")).bind(personId, actor.companyId).first<{ branch_id: string | null; department_id: string | null }>(),
    canManagePeople,
    money: decimalToMinor,
    async prepareDraft(actor, key, input, facts, capturedAuthority) {
      const { recordId, current, claimant, claimantName, personId, staff, amount, tax, category, currency, requestKind, payeePartyId } = facts;
      const authority = capturedAuthority as Awaited<ReturnType<typeof expenseAuthoritySnapshot>>;
      const now = Date.now(), version = current ? input.expectedVersion + 1 : 1;
      const steps: SqlStatement[] = [
    authority.guard(key),
    assertion(
      write,
      key,
      "claimant-access",
      `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE user_id=? AND company_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>?)`,
      [claimant, actor.companyId, now, now],
    ),
  ];

  if(payeePartyId)steps.push(assertion(write,key,"supplier-payee",ppRd29FinancePartySupplierSql(financeProjectionSources.finance_parties),[payeePartyId,actor.companyId,currency]));
  if (category)
    steps.push(
      assertion(
        write,
        key,
        "category",
        ppRd51AccountingCategoryGuard(true, 'nullable-both'),
        [category, actor.companyId, currency, currency, currency],
      ),
    );
  if (current)
    steps.push(
      assertion(
        write,
        key,
        "expense-version",
        PP_RD23_FINANCE_EXPENSE_DRAFT_REJECTED_GUARD,
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
  steps.push(await expenseDrafts(write, actor.companyId).write(!current, values, recordId, actor.userId, now));

      return expenseMutation({ id: recordId, status: "draft", version }, steps);
    },
  });
}
export async function saveExpenseDraft(actor: WorkContext, id: string | null, input: z.infer<typeof expenseUpdateSchema>, key: string) {
  const { write } = await holdingResources(actor.holdingId);
  return withExpensePurchasedCommand(write, actor, {key,purpose:"new-work"}, tx => nativeExpenseDrafts(tx)(actor, id, input, key));
}

export function nativeExpenseDrafts(database: SqlDatabase){return (...args:Parameters<ReturnType<typeof originalNativeExpenseDrafts>>)=>withExpensePurchasedCommand(database,args[0],{key:args[3],purpose:"new-work"},transaction=>originalNativeExpenseDrafts(transaction)(...args));}
