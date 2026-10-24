import{canAccountExpenses,canPostExpenses,visibility,currentExpenseReader,approvalFields,expenseSelect,originalNativeExpenseReads,listExpenses,getExpense,expenseFromDatabase,canEditExpense,canReviewExpense,nativeExpenseReads,expensePresentationRows,expenseVisibleRecordIds,type ExpenseRow}from '@erpx/server-people_payroll/adapters/expense-read';
export{canAccountExpenses,canPostExpenses,visibility,currentExpenseReader,approvalFields,expenseSelect,originalNativeExpenseReads,listExpenses,getExpense,expenseFromDatabase,canEditExpense,canReviewExpense,nativeExpenseReads,expensePresentationRows,expenseVisibleRecordIds};
export type{ExpenseRow};
import {expensePaymentCancellations,expensePreparedPaymentSources,expensePaymentTreasurySql} from '@erpx/server-people_payroll/adapters/expense-payment-cancellation-sources';
import {requireExpensePortUnit} from "@erpx/server-people_payroll/adapters/expense-purchases";

import {withExpensePurchasedCommand, withExpensePurchasedRead} from "@erpx/server-people_payroll/adapters/expense-purchases";
import { workflowDecisionEffects } from "@erpx/server-workflow/public/decision-effects";
import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "@erpx/server-commerce/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as financeProjectionSources } from "@erpx/server-finance/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as workflowProjectionSources } from "@erpx/server-workflow/public/hospitality-owner-projection-sources";
import { expenseNativeEffects as filesExpenseEffects } from "@erpx/server-files/public/expense-native-effects";
import { expenseNativeEffects as financeExpenseEffects } from "@erpx/server-finance/public/expense-native-effects";
import { createExpenseDetail, createExpenseOptions } from "@erpx/server-people_payroll/application/expense-projections";
import { authorizeExpenseReversal, createTransitionExpense, createPrepareExpensePosting, createReverseExpense } from "@erpx/server-people_payroll/application/expense-lifecycle";

import { createRevokeExpenseAttachment } from "@erpx/server-people_payroll/application/expense-evidence";
import { createPostExpense } from "@erpx/server-people_payroll/application/expense-commands";
import { expenseCommandPort, expenseMutation, expenseStatements } from "@erpx/server-people_payroll/adapters/expense-command-port";
/** Native Expense adapter; preserves legacy persistence and command semantics. */
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";

import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import { canManagePeople } from "@erpx/server-people_payroll/public/people-directory";
import {prepareJournal} from '@erpx/server-finance/public/journal-planning';
import { sha256 } from "@erpx/server-runtime/public/crypto";
import { ApiError } from "@erpx/server-runtime/public/http";
import {departmentRoute} from '@erpx/server-organization/public/department-authority-reads';

import { staffGuard } from "@erpx/server-operations/public/staff-policy";
import {approvalView,openApproval,prepareDecision,closeApprovals} from '@erpx/server-workflow/public/approval-planning';

import { expensePayers } from '@erpx/server-people_payroll/adapters/expense-workflow-read';
import {nativeExpenseTransitions} from '@erpx/server-people_payroll/adapters/expense-workflow';
import {
  attachmentsV2Readable,
  attachmentsV2ReadOnly,
} from "@erpx/server-files/public/attachments/feature";
export {
  expenseDraftSchema,
  expenseUpdateSchema,
  saveExpenseDraft as saveExpense,
} from "@erpx/server-people_payroll/adapters/expense-drafts";

export const expenseDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
  override: z.boolean().default(false),
});
export const expensePostSchema = z.object({
  expectedVersion: z.number().int().positive(),
  periodId: z.string().min(1),
  entryDate: z.iso.date(),
});
export const expenseVersionSchema = z.object({
  expectedVersion: z.number().int().positive(),
});

/** Bind persistence, visibility and actor once, including transaction-owned reads. */

/** Bound owner read reused by commands inside their caller's transaction. */

function originalNativeExpenseDetail(write: SqlDatabase) {
  return createExpenseDetail({
    expense: (actor, id) => expenseFromDatabase(write, actor, id),
    async attachments(_actor, id) { return (await write
      .prepare(
        attachmentsV2Readable()
          ? `SELECT DISTINCT a.id,a.original_name,a.media_type,a.bytes FROM (${filesProjectionSources.attachments}) a
             JOIN (${filesProjectionSources.attachment_links}) l ON l.attachment_id=a.id JOIN (${filesProjectionSources.attachment_sets}) s ON s.id=l.set_id
             WHERE a.owner_type='expense_claim' AND a.owner_id=? AND a.revoked_at IS NULL
               AND a.source_state='available' AND l.state='bound'
               AND s.target_type='expense_claim' AND s.target_id=a.owner_id ORDER BY a.id`
          : `SELECT id,original_name,media_type,bytes FROM (${filesProjectionSources.attachments}) attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL ORDER BY created_at`,
      )
      .bind(id)
      .all<ExpenseRow>()).results; },
    async history(actor, id, expense) { return (await write
      .prepare(
        `SELECT a.id,a.action,a.created_at,COALESCE(p.legal_name,a.actor_id) actor_name FROM (${workflowProjectionSources.audit_events}) a LEFT JOIN hr_people p ON p.user_id=a.actor_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE a.record_id IN (?,?) ORDER BY a.created_at DESC LIMIT 50`,
      )
      .bind(actor.companyId, id, expense.invoice_id)
      .all<ExpenseRow>()).results; },
    async payments(_actor, expense) { return (await write
      .prepare(
        `SELECT p.id,p.payment_number,p.payment_date,p.currency,a.amount_minor FROM (${financeProjectionSources.finance_allocations}) a JOIN (${financeProjectionSources.finance_payments}) p ON p.id=a.payment_id WHERE a.invoice_id=? ORDER BY p.payment_date`,
      )
      .bind(expense.invoice_id)
      .all<ExpenseRow>()).results; },
    async decisions(actor, id) { return (await write
      .prepare(
        "SELECT d.*,COALESCE(p.legal_name,d.actor_id) actor_name FROM expense_decisions d LEFT JOIN hr_people p ON p.user_id=d.actor_id AND p.company_id=? AND p.merged_into_id IS NULL WHERE d.expense_id=? ORDER BY d.created_at DESC",
      )
      .bind(actor.companyId, id)
      .all<ExpenseRow>()).results; },
    async paymentDrafts(actor, id) { return (await write
      .prepare(
        `SELECT d.*,CASE WHEN EXISTS(SELECT 1 FROM (${expensePreparedPaymentSources}) native WHERE native.id=d.id AND native.company_id=d.company_id) THEN 1 ELSE 0 END native_preparation,
         terminal.reason cancellation_reason FROM expense_payment_drafts d LEFT JOIN (${expensePaymentCancellations}) terminal ON terminal.id=d.id AND terminal.company_id=d.company_id
         WHERE d.expense_id=? AND d.company_id=? ORDER BY d.created_at`,
      )
      .bind(id, actor.companyId)
      .all<ExpenseRow>()).results; },
    async paymentAttachments(actor, id) { return (await write
      .prepare(
        attachmentsV2Readable()
          ? `SELECT DISTINCT a.id,a.owner_id,a.original_name,a.media_type,a.bytes FROM (${filesProjectionSources.attachments}) a
             JOIN expense_payment_drafts d ON d.id=a.owner_id
             JOIN (${filesProjectionSources.attachment_links}) l ON l.attachment_id=a.id JOIN (${filesProjectionSources.attachment_sets}) s ON s.id=l.set_id
             WHERE a.owner_type='expense_payment' AND d.expense_id=? AND d.company_id=? AND a.revoked_at IS NULL
               AND a.source_state='available' AND l.state${attachmentsV2ReadOnly() ? "='bound'" : " IN ('draft','bound')"}
               AND s.target_type='expense_payment' AND s.target_id=a.owner_id ORDER BY a.id`
          : `SELECT a.id,a.owner_id,a.original_name,a.media_type,a.bytes FROM (${filesProjectionSources.attachments}) a JOIN expense_payment_drafts d ON d.id=a.owner_id WHERE a.owner_type='expense_payment' AND d.expense_id=? AND d.company_id=? AND a.revoked_at IS NULL ORDER BY a.created_at`,
      )
      .bind(id, actor.companyId)
      .all<ExpenseRow>()).results; },
    approval: (actor, id) => approvalView(write, "expense", id, actor),
  });
}
export async function expenseDetail(actor: WorkContext, id: string, provided?: SqlDatabase) {
  const write = provided ?? (await holdingResources(actor.holdingId)).write;
  return withExpensePurchasedRead(write, actor, tx => nativeExpenseDetail(tx)(actor, id));
}
function originalNativeExpenseOptions(write: SqlDatabase) {
  return createExpenseOptions({
    async people(actor) { return (await write
      .prepare(
        `SELECT p.id,p.legal_name,p.user_id,e.branch_id FROM hr_people p JOIN hr_engagements e ON e.id=(SELECT id FROM hr_engagements WHERE person_id=p.id AND status='active' ORDER BY start_date DESC LIMIT 1)
      WHERE p.company_id=? AND p.status='active' AND p.merged_into_id IS NULL AND (p.user_id=? OR ((?::text IS NULL OR e.branch_id=?) AND (?::text IS NULL OR e.department_id=?) AND ?=1)) ORDER BY p.legal_name`,
      )
      .bind(
        actor.companyId,
        actor.userId,
        actor.branchId,
        actor.branchId,
        actor.departmentId,
        actor.departmentId,
        canManagePeople(actor) ? 1 : 0,
      )
      .all<ExpenseRow>()).results; },
    async accounts(actor) { return (await write
      .prepare(
        `SELECT a.id,a.name,a.code,a.currency FROM (${financeProjectionSources.finance_accounts}) a JOIN (${commerceProjectionSources.commerce_settings}) s ON s.chart_id=a.chart_id AND s.company_id=a.company_id WHERE a.company_id=? AND a.category='expense' AND a.status='active' ORDER BY CASE WHEN a.code='5100' THEN 0 ELSE 1 END,a.code`,
      )
      .bind(actor.companyId)
      .all<ExpenseRow>()).results; },
    async periods(actor) { return (await write
      .prepare(
        `SELECT id,name,start_date,end_date FROM (${financeProjectionSources.finance_periods}) finance_periods WHERE company_id=? AND state='open' AND EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE company_id=? AND id=? AND iam_role_allows(role,company_id,'finance.period.select') AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>?) ORDER BY start_date DESC`,
      )
      .bind(
        actor.companyId,
        actor.companyId,
        actor.assignmentId,
        Date.now(),
        Date.now(),
      )
      .all<ExpenseRow>()).results; },
    async payees(actor) { return (await write.prepare(`SELECT id,name,currency FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE company_id=? AND kind='supplier' AND status='active' ORDER BY name`).bind(actor.companyId).all<ExpenseRow>()).results; },
  });
}
export async function expenseOptions(actor: WorkContext) {
  const { write } = await holdingResources(actor.holdingId);
  return withExpensePurchasedRead(write, actor, tx => nativeExpenseOptions(tx)(actor));
}
function independentClaimant(
  write: SqlDatabase,
  key: string,
  id: string,
  actor: WorkContext,
) {
  return assertion(
    write,
    key,
    "independent-claimant",
    "EXISTS(SELECT 1 FROM finance_expenses x JOIN hr_people p ON p.id=x.employee_id WHERE x.id=? AND p.user_id IS DISTINCT FROM ?)",
    [id, actor.userId],
  );
}
function versionGuard(
  write: SqlDatabase,
  key: string,
  id: string,
  actor: WorkContext,
  version: number,
  states: string[],
) {
  return assertion(
    write,
    key,
    "expense-version",
    `EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND company_id=? AND version=? AND cancelled_at IS NULL AND status IN (${states.map(() => "?").join(",")}))`,
    [id, actor.companyId, version, ...states],
  );
}
function originalNativeExpenseLifecycle(write: SqlDatabase) {
  return createTransitionExpense({
    ...expenseCommandPort(write),
    expense: (actor, id, override) => override ? write.prepare("SELECT * FROM finance_expenses WHERE id=? AND company_id=?").bind(id, actor.companyId).first<ExpenseRow>() : expenseFromDatabase(write, actor, id),
    current: (actor, row, action, input, key) => nativeExpenseTransitions(write)(actor, row, action, input, key),
    async canOverride(actor, id) { return (await approvalView(write, "expense", id, actor)).canOverride; },
    async decision(actor, id, action, input, key) {
      const native = await prepareDecision(write, actor, key, "expense", id, action === "approve", input.reason ?? "", input.expectedVersion, input.override);
      return { final: native.final, native };
    },
    async prepareLegacy(actor, id, row, action, input, key, capturedDecision, capturedRoute, capturedOpened, status) {
      const decision = capturedDecision?.native as Awaited<ReturnType<typeof prepareDecision>> | null;
      const route = capturedRoute?.native as Awaited<ReturnType<typeof departmentRoute>> | null;
      const opened = capturedOpened as Awaited<ReturnType<typeof openApproval>> | null;
      const review = action === "approve" || action === "reject", operation = `expense.${action}`, now = Date.now();
      const steps = [
    versionGuard(
      write,
      key,
      id,
      actor,
      input.expectedVersion,
      review
        ? ["in_review"]
        : action === "cancel"
          ? ["draft", "rejected", "in_review"]
          : ["draft", "rejected"],
    ),
    ...(opened?.statements ?? []),
    ...(decision?.statements ?? []),
    ...(action === "cancel"
      ? closeApprovals(
          write,
          key,
          "expense",
          id,
          actor.userId,
          "cancel",
          input.reason ?? "",
          input.expectedVersion,
        )
      : []),
    ...(action === "submit"
      ? [
          staffGuard(write, key, actor.companyId, String(row.claimant_user_id)),
          assertion(
            write,
            key,
            "receipt",
            attachmentsV2Readable()
              ? `EXISTS(SELECT 1 FROM (${filesProjectionSources.attachments}) a JOIN (${filesProjectionSources.attachment_links}) l ON l.attachment_id=a.id JOIN (${filesProjectionSources.attachment_sets}) s ON s.id=l.set_id
                  WHERE a.owner_type='expense_claim' AND a.owner_id=? AND a.revoked_at IS NULL AND a.source_state='available'
                    AND l.state='bound' AND s.target_type='expense_claim' AND s.target_id=a.owner_id)`
              : `EXISTS(SELECT 1 FROM (${filesProjectionSources.attachments}) attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL)`,
            [id],
          ),
          assertion(
            write,
            key,
            "required-inputs",
            "EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND length(business_purpose)>=5 AND length(receipt_reference)>=2)",
            [id],
          ),
        ]
      : []),
    ...(route
      ? [
          write
            .prepare(
              "UPDATE finance_expenses SET branch_id=?,department_id=? WHERE id=?",
            )
            .bind(route.scope.branch_id, route.scope.department_id, id),
        ]
      : []),
    write
      .prepare(
        "UPDATE finance_expenses SET status=?,submitted_at=CASE WHEN ?='submit' THEN ? ELSE submitted_at END,approved_by=CASE WHEN ?='approve' THEN ? WHEN ?='submit' THEN NULL ELSE approved_by END,rejection_reason=?,cancelled_at=CASE WHEN ?='cancel' THEN ? ELSE cancelled_at END,version=version+1,updated_at=? WHERE id=?",
      )
      .bind(
        status,
        action,
        now,
        action,
        actor.userId,
        action,
        action === "reject" || action === "cancel"
          ? (input.reason ?? null)
          : null,
        action,
        now,
        now,
        id,
      ),
    ...(review ? [independentClaimant(write, key, id, actor)] : []),
    ...(action !== "submit"
      ? [
          write
            .prepare(
              "INSERT INTO expense_decisions(id,expense_id,command_id,action,actor_id,reviewed_version,reason,created_at) VALUES(?,?,?,?,?,?,?,?)",
            )
            .bind(
              crypto.randomUUID(),
              id,
              key,
              action,
              actor.userId,
              input.expectedVersion,
              input.reason!,
              now,
            ),
          workflowDecisionEffects(write).appendReviewedDecision({
            id: crypto.randomUUID(),
            commandId: key,
            recordId: id,
            action: operation,
            actorId: actor.userId,
            assignmentId: actor.assignmentId,
            reviewedVersion: input.expectedVersion,
            inputDigest: await sha256(JSON.stringify(input)),
            createdAt: now
          }),
        ]
      : []),
  ];
      return expenseMutation({
      id,
      status: action === "cancel" ? "cancelled" : status,
      version: input.expectedVersion + 1,
    }, steps);
    },
  });
}
export async function transitionExpense(actor: WorkContext, id: string, action: "submit" | "approve" | "reject" | "cancel", input: { expectedVersion: number; reason?: string; override?: boolean }, key: string) {
  const { write } = await holdingResources(actor.holdingId);
  return withExpensePurchasedCommand(write, actor, {key,purpose:action === "submit" ? "new-work" : "settle-existing"}, tx => nativeExpenseLifecycle(tx)(actor, id, action, input, key));
}
function originalNativeExpenseReceiptRemoval(write: SqlDatabase) {
  return createRevokeExpenseAttachment({
    ...expenseCommandPort(write),
    readOnly: attachmentsV2ReadOnly,
    expense: (actor, id) => expenseFromDatabase(write, actor, id),
    link: (actor, id, attachmentId) => attachmentsV2Readable()
      ? write.prepare(`SELECT l.id,l.state,l.set_id FROM (${filesProjectionSources.attachment_links}) l JOIN (${filesProjectionSources.attachment_sets}) s ON s.id=l.set_id WHERE l.attachment_id=? AND s.company_id=? AND s.target_type='expense_claim' AND s.target_id=?`)
        .bind(attachmentId, actor.companyId, id).first<{ id: string; state: string; set_id: string }>()
      : Promise.resolve(undefined),
    draftLink: (actor, id, attachmentId) => attachmentsV2Readable()
      ? write.prepare(`SELECT l.id,l.set_id FROM (${filesProjectionSources.attachment_links}) l JOIN (${filesProjectionSources.attachment_sets}) s ON s.id=l.set_id WHERE l.attachment_id=? AND s.company_id=? AND s.target_type='expense_claim' AND s.target_id=? AND l.state='draft'`)
        .bind(attachmentId, actor.companyId, id).first<{ id: string; set_id: string }>()
      : Promise.resolve(null),
    async prepareRemoval(actor, key, id, attachmentId, expectedVersion, expense, canonical) {
      const now = Date.now();
      return expenseMutation({ id, status: expense.status, version: expectedVersion + 1 }, [
      versionGuard(write, key, id, actor, expectedVersion, [
        "draft",
        "rejected",
      ]),
      assertion(
        write,
        key,
        "receipt",
        `EXISTS(SELECT 1 FROM (${filesProjectionSources.attachments}) attachments WHERE id=? AND owner_id=? AND owner_type='expense_claim' AND revoked_at IS NULL)`,
        [attachmentId, id],
      ),
      ...(canonical
        ? [
            assertion(
              write,
              key,
              "canonical-receipt-draft",
              `EXISTS(SELECT 1 FROM (${filesProjectionSources.attachment_links}) attachment_links WHERE id=? AND state='draft')`,
              [canonical.id],
            ),
            filesExpenseEffects(write).withdrawLink({at:now,actorId:actor.userId,reason:"Removed from expense preparation",linkId:canonical.id}),
            filesExpenseEffects(write).withdrawEvent({id:`attachment-event-${crypto.randomUUID()}`,companyId:actor.companyId,attachmentId:attachmentId,linkId:canonical.id,setId:canonical.set_id,actorId:actor.userId,details:JSON.stringify({ reason: "Removed from expense preparation" }),at:now}),
          ]
        : []),
      filesExpenseEffects(write).revokeAttachment({at:now,attachmentId:attachmentId}),
      write
        .prepare(
          "UPDATE finance_expenses SET version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, id),
    ]);
    },
  });
}
export async function revokeExpenseAttachment(actor: WorkContext, id: string, attachmentId: string, expectedVersion: number, key: string) {
  const { write } = await holdingResources(actor.holdingId);
  return withExpensePurchasedCommand(write, actor, {key,purpose:"settle-existing"}, tx => nativeExpenseReceiptRemoval(tx)(actor, id, attachmentId, expectedVersion, key));
}
function originalNativeExpensePosting(write: SqlDatabase) {
  return createPostExpense({
    ...expenseCommandPort(write),
    expense: (actor, id) => expenseFromDatabase(write, actor, id),
    legacyPoster: actor => requirePermission(actor, "expense.approve"),
    async preparePosting(actor, claim, input, key) {
      const prepared = await prepareExpensePosting(write, actor, claim, input, key);
      return expenseMutation(prepared.result, prepared.steps);
    },
  });
}
export async function postExpense(actor: WorkContext, id: string, input: z.infer<typeof expensePostSchema>, key: string) {
  const { write } = await holdingResources(actor.holdingId);
  return withExpensePurchasedCommand(write, actor, {key,purpose:"settle-existing"}, tx => nativeExpensePosting(tx)(actor, id, input, key));
}
export function nativeExpensePostingPreparation(write: SqlDatabase) {
  return createPrepareExpensePosting({
    error: violation => new ApiError(violation.status, violation.code, violation.message),
    mapping: (actor, claim) => write.prepare(`SELECT s.* FROM (${commerceProjectionSources.commerce_settings}) s JOIN (${financeProjectionSources.finance_accounts}) a ON a.chart_id=s.chart_id WHERE s.company_id=? AND a.id=? AND s.currency=?`).bind(actor.companyId, claim.category_account_id, claim.currency).first<ExpenseRow>(),
    async journal(actor, input, key) { const native = await prepareJournal(write, actor, input, key, "posted"); return { id: native.result.id, native }; },
    async preparePosting(actor, claim, input, key, config, total, invoice, payee, preparedJournal) {
      const journal = preparedJournal.native as Awaited<ReturnType<typeof prepareJournal>>;
      const id = String(claim.id), now = Date.now();
      const postedMoney = await financeExpenseEffects(write).preparePostedMoney(actor, {
        journalId: journal.result.id, invoiceId: invoice, transactionDate: input.entryDate,
        currency: String(claim.currency), totalMinor: total,
      });
      return expenseMutation({
      id,
      status: "posted",
      version: input.expectedVersion + 1,
      invoiceId: invoice,
      journalId: journal.result.id,
    }, [
      versionGuard(
        write,
        key,
        id,
        actor,
        input.expectedVersion,
        ["approved"],
      ),
      independentClaimant(write, key, id, actor),
      assertion(
        write,
        key,
        "expense-account",
        `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_accounts}) finance_accounts WHERE id=? AND company_id=? AND category='expense')`,
        [claim.category_account_id, actor.companyId],
      ),
      assertion(
        write,
        key,
        "receipt",
            attachmentsV2Readable()
              ? `EXISTS(SELECT 1 FROM (${filesProjectionSources.attachments}) a JOIN (${filesProjectionSources.attachment_links}) l ON l.attachment_id=a.id JOIN (${filesProjectionSources.attachment_sets}) s ON s.id=l.set_id
                  WHERE a.owner_type='expense_claim' AND a.owner_id=? AND a.revoked_at IS NULL AND a.source_state='available'
                    AND l.state='bound' AND s.target_type='expense_claim' AND s.target_id=a.owner_id)`
              : `EXISTS(SELECT 1 FROM (${filesProjectionSources.attachments}) attachments WHERE owner_type='expense_claim' AND owner_id=? AND revoked_at IS NULL)`,
        [id],
      ),
      ...(claim.request_kind==="payment"&&claim.payee_party_id ? [assertion(write,key,"payee",`EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND kind='supplier' AND status='active' AND currency=?)`,[payee,actor.companyId,claim.currency])] : [
      financeExpenseEffects(write).retainEmployeeParty({id:payee,companyId:actor.companyId,code:`EMP-${claim.employee_id}`,name:claim.legal_name,currency:claim.currency,personId:claim.employee_id,createdAt:now,updatedAt:now}),
      assertion(
        write,
        key,
        "payee",
        `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_parties}) finance_parties WHERE id=? AND company_id=? AND person_id=? AND currency=? AND status='active')`,
        [payee, actor.companyId, claim.employee_id, claim.currency],
      ),
      ]),
      ...journal.steps,
      financeExpenseEffects(write).postPayableInvoice({id:invoice,companyId:actor.companyId,partyId:payee,number:`EXP-${(await sha256(id)).slice(0, 24).toUpperCase()}`,invoiceDate:input.entryDate,dueDate:input.entryDate,currency:claim.currency,subtotalMinor:claim.amount_minor,taxMinor:claim.tax_minor,totalMinor:total,outstandingMinor:total,debitAccountId:claim.category_account_id,creditAccountId:config.payable_account_id,taxAccountId:config.input_tax_account_id,expenseId:id,journalId:journal.result.id,createdBy:claim.prepared_by ?? claim.created_by,approvedBy:claim.approved_by,postedAt:now,createdAt:now,updatedAt:now}),
      ...postedMoney,
      write
        .prepare(
          "UPDATE finance_expenses SET status='posted',workflow_stage='payment',invoice_id=?,journal_id=?,posted_at=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(invoice, journal.result.id, now, now, id),
    ]);
    },
  });
}
export async function prepareExpensePosting(write: SqlDatabase, actor: WorkContext, claim: ExpenseRow, input: z.infer<typeof expensePostSchema>, key: string) {
  await requireExpensePortUnit(write,actor,key,"expense.post");
  const prepared = await nativeExpensePostingPreparation(write)(actor, claim, input, key);
  return { result: prepared.result, steps: expenseStatements(prepared) };
}

export const expenseReversalSchema = expensePostSchema.extend({
  reason: z.string().trim().min(3).max(2000),
});
function originalNativeExpenseReversal(write: SqlDatabase) {
  return createReverseExpense({
    ...expenseCommandPort(write),
    authorize: actor => requirePermission(actor, "expense.approve"),
    expense: (actor, id) => expenseFromDatabase(write, actor, id),
    invoice: (actor, id, claim) => write.prepare(`SELECT * FROM (${financeProjectionSources.finance_invoices}) finance_invoices WHERE id=? AND company_id=? AND source_type='expense_claim' AND source_id=?`).bind(claim.invoice_id, actor.companyId, id).first<ExpenseRow>(),
    journal: (actor, id, claim) => write.prepare(`SELECT * FROM (${financeProjectionSources.finance_journals}) finance_journals WHERE id=? AND company_id=? AND source_type='expense_claim' AND source_id=? AND status='posted'`).bind(claim.journal_id, actor.companyId, id).first<ExpenseRow>(),
    async prepareReversal(actor, id, input, key, claim, invoice, journal) {
      const reversal = `expense-reversal-${key}`, now = Date.now();
      const reversalMoney = await financeExpenseEffects(write).prepareReversalMoney(actor, {
        journalId: reversal, originalJournalId: String(journal.id), transactionDate: input.entryDate,
        currency: String(journal.currency), totalMinor: Number(invoice.total_minor),
      });
      return expenseMutation({
      id,
      status: "cancelled",
      version: input.expectedVersion + 1,
      reversalJournalId: reversal,
    }, [
      versionGuard(write, key, id, actor, input.expectedVersion, ["posted"]),
      independentClaimant(write, key, id, actor),
      assertion(
        write,
        key,
        "unpaid",
        `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_invoices}) finance_invoices WHERE id=? AND status='posted' AND version=? AND outstanding_minor=total_minor) AND NOT EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_allocations}) finance_allocations WHERE invoice_id=?)`,
        [invoice.id, invoice.version, invoice.id],
      ),
      assertion(
        write,
        key,
        "no-export",
        "NOT EXISTS(SELECT 1 FROM hr_payment_batches WHERE source_type='expense' AND source_id=? AND status!='cancelled')",
        [id],
      ),
      assertion(
        write,
        key,
        "journal",
        `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_journals}) finance_journals WHERE id=? AND status='posted' AND version=?)`,
        [journal.id, journal.version],
      ),
      assertion(
        write,
        key,
        "period",
        `EXISTS(SELECT 1 FROM (${financeProjectionSources.finance_periods}) finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?)`,
        [input.periodId, actor.companyId, input.entryDate, input.entryDate],
      ),
      financeExpenseEffects(write).retainReversalJournal({id:reversal,periodId:input.periodId,number:`REV-${key}`,date:input.entryDate,expenseId:id,reason:input.reason,createdBy:actor.userId,postedBy:actor.userId,postedAt:now,createdAt:now,updatedAt:now,originalJournalId:journal.id}),
      financeExpenseEffects(write).retainReversalLines({prefix:reversal,journalId:reversal,originalJournalId:journal.id}),
      reversalMoney,
      financeExpenseEffects(write).markJournalReversed({actorId:actor.userId,reversedAt:now,updatedAt:now,journalId:journal.id}),
      financeExpenseEffects(write).voidExpenseInvoice({updatedAt:now,invoiceId:invoice.id}),
      write
        .prepare(
          "UPDATE finance_expenses SET status='rejected',cancelled_at=?,rejection_reason=?,reversal_journal_id=?,version=version+1,updated_at=? WHERE id=?",
        )
        .bind(now, input.reason, reversal, now, id),
      write
        .prepare(
          "INSERT INTO expense_decisions(id,expense_id,command_id,action,actor_id,reviewed_version,reason,created_at) VALUES(?,?,?,'cancel',?,?,?,?)",
        )
        .bind(
          crypto.randomUUID(),
          id,
          key,
          actor.userId,
          input.expectedVersion,
          input.reason,
          now,
        ),
    ]);
    },
  });
}
export async function reverseExpense(actor: WorkContext, id: string, input: z.infer<typeof expenseReversalSchema>, key: string) {
  // Resolve resources after the same authorization used by the legacy entry.
  actor = authorizeExpenseReversal(actor, { authorize: value => requirePermission(value, "expense.approve"), error: violation => new ApiError(violation.status, violation.code, violation.message) });
  const { write } = await holdingResources(actor.holdingId);
  return withExpensePurchasedCommand(write, actor, {key,purpose:"settle-existing"}, tx => nativeExpenseReversal(tx)(actor, id, input, key));
}

export function nativeExpenseLifecycle(database: SqlDatabase){return (...args:Parameters<ReturnType<typeof originalNativeExpenseLifecycle>>)=>withExpensePurchasedCommand(database,args[0],{key:args[4],purpose:args[2] === "submit" ? "new-work" : "settle-existing"},transaction=>originalNativeExpenseLifecycle(transaction)(...args));}

export function nativeExpenseReceiptRemoval(database: SqlDatabase){return (...args:Parameters<ReturnType<typeof originalNativeExpenseReceiptRemoval>>)=>withExpensePurchasedCommand(database,args[0],{key:args[4],purpose:"settle-existing"},transaction=>originalNativeExpenseReceiptRemoval(transaction)(...args));}

export function nativeExpensePosting(database: SqlDatabase){return (...args:Parameters<ReturnType<typeof originalNativeExpensePosting>>)=>withExpensePurchasedCommand(database,args[0],{key:args[3],purpose:"settle-existing"},transaction=>originalNativeExpensePosting(transaction)(...args));}

export function nativeExpenseReversal(database: SqlDatabase){return (...args:Parameters<ReturnType<typeof originalNativeExpenseReversal>>)=>withExpensePurchasedCommand(database,args[0],{key:args[3],purpose:"settle-existing"},transaction=>originalNativeExpenseReversal(transaction)(...args));}

export function nativeExpenseDetail(database:SqlDatabase){return (...args:Parameters<ReturnType<typeof originalNativeExpenseDetail>>)=>withExpensePurchasedRead(database,args[0],async transaction=>{
 const detail=await originalNativeExpenseDetail(transaction)(...args),actor=args[0],expense=detail.expense;
 const canCancel=expense.workflow_version===2&&['posted','reimbursed','cancelled'].includes(String(expense.status))&&!!expense.invoice_id&&!!expense.journal_id&&hasPermission(actor,'expense.pay')&&expense.expense_purchase_settle_existing!==0&&(await expensePayers(transaction,expense)).includes(actor.userId)&&Boolean(await transaction.prepare(`SELECT EXISTS(SELECT 1 FROM finance_expenses e WHERE e.id=? AND e.company_id=? AND (${expensePaymentTreasurySql('e')})) ok`).bind(expense.id,actor.companyId,actor.assignmentId,actor.userId).first<boolean>('ok'));
 return {...detail,paymentDrafts:detail.paymentDrafts.map((draft):ExpenseRow & {can_cancel_payment:number}=>({...draft,can_cancel_payment:canCancel&&draft.native_preparation===1&&draft.status==='draft'&&draft.payment_id===null?1:0})),permittedActions:detail.permittedActions.filter(action=>['save','submit'].includes(action)?expense.expense_purchase_new_work!==0:expense.expense_purchase_settle_existing!==0)};
});}
export function nativeExpenseOptions(database:SqlDatabase){return (...args:Parameters<ReturnType<typeof originalNativeExpenseOptions>>)=>withExpensePurchasedRead(database,args[0],transaction=>originalNativeExpenseOptions(transaction)(...args));}

/** Shared-record candidates use the original two-stage native reader, before
 * their caller applies pagination. Only the owned read seam supplies IDs.
 * No decoration or legacy initialization is needed to decide visibility. */
