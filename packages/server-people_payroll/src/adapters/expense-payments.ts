import { ppRd21IdVersionStatusGuard } from './pp-read-tail-r21-sources';
import { ppRd27IdCompanyVersionStatusLiteralGuard } from './pp-read-tail-r27-sources';
import { ppRd48IdCompanyVersionPairGuard, ppRd48ExpenseClaimInvoice } from './pp-read-tail-r48-sources';
import { ppRd49AttachmentLinkPoint, ppRd49DraftLinkGuard } from './pp-read-tail-r49-sources';
import { ppRd50SlipOwnerGuard } from './pp-read-tail-r50-sources';
import { ppRd55AttachmentPresenceV2, ppRd55AttachmentPresenceV1 } from './pp-read-tail-r55-sources';
import { nativePpStatement } from '@erpx/server-people_payroll/adapters/native-people-payroll-command';
import{paymentDraft,nativeExpensePaymentReader,paymentFromDatabase}from '@erpx/server-people_payroll/adapters/expense-payment-read';
export{paymentDraft,nativeExpensePaymentReader,paymentFromDatabase};

import { withExpensePurchasedCommand } from "@erpx/server-people_payroll/adapters/expense-purchases";
import { hospitalityOwnerProjectionSources as filesProjectionSources } from "@erpx/server-files/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as organizationProjectionSources } from "@erpx/server-organization/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as workflowProjectionSources } from "@erpx/server-workflow/public/hospitality-owner-projection-sources";
import {expenseNativeEffects as filesExpenseEffects} from '@erpx/server-collaboration/adapters/native-expense-effects';

import { createSaveExpensePayment, createRevokePaymentSlip } from "@erpx/server-people_payroll/application/expense-evidence";
import { createExpensePaymentCommands } from "@erpx/server-people_payroll/application/expense-commands";
import { expenseCommandPort, expenseMutation } from "@erpx/server-people_payroll/adapters/expense-command-port";
/** Native Expense adapter; preserves legacy persistence and command semantics. */
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import "server-only";
import { z } from "zod";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import {expenseFromDatabase} from '@erpx/server-people_payroll/adapters/expense-read';
import {type ExpenseRow} from '@erpx/server-people_payroll/adapters/expenses';
import {expensePayers} from '@erpx/server-people_payroll/adapters/expense-workflow-read';

import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {prepareInvoiceSettlement} from '@erpx/server-commerce/public/commerce';
import { notification, staffGuard } from "@erpx/server-operations/public/staff-policy";
import {decimalToMinor} from '@erpx/server-finance/public/money-primitives';
import {
  attachmentsV2Enabled,
  attachmentsV2Readable,
  attachmentsV2ReadOnly,
} from "@erpx/server-files/public/attachments/feature";
import { prepareAttachmentBinding } from "@erpx/server-files/public/attachments/service";

function paymentSlipExistsSql() {
  return attachmentsV2Readable()
    ? ppRd55AttachmentPresenceV2('exists', true, true, false)
    : ppRd55AttachmentPresenceV1('exists', true, true, false);
}

async function preparePaymentEvidence(
  db: SqlDatabase,
  actor: WorkContext,
  id: string,
  expectedVersion: number,
  commandId: string,
  bindingType: string,
) {
  if (!attachmentsV2Enabled()) return null;
  const setIds = (
    await db
      .prepare(
        `SELECT id FROM (${filesProjectionSources.attachment_sets}) attachment_sets WHERE company_id=? AND target_type='expense_payment' AND target_id=?
           AND component_kind='action' AND purpose_key='payment_slip'
           AND expected_target_version=? AND state='draft'
         ORDER BY created_at,id`,
      )
      .bind(actor.companyId, id, expectedVersion)
      .all<{ id: string }>()
  ).results.map((row) => row.id);
  if (!setIds.length) return null;
  return prepareAttachmentBinding(db, actor, {
    attachmentSetIds: setIds,
    bindingContract: {
      targetType: "expense_payment",
      bindingType,
      contexts: [{ purposeKey: "payment_slip", componentKind: "action", componentRef: "legacy-upload-adapter" }],
    },
    targetId: id,
    expectedTargetVersion: expectedVersion,
    commandId,
    bindingId: id,
  });
}

export const expensePaymentSchema = z.object({
  expectedVersion: z.number().int().positive(),
  date: z.iso.date(),
  amount: z.string().regex(/^\d+(\.\d{1,2})?$/),
  reference: z.string().trim().min(2).max(100),
});

export async function createExpensePayment(actor: WorkContext, id: string, expectedVersion: number, key: string) {
  const { write } = await holdingResources(actor.holdingId);
  return withExpensePurchasedCommand(write, actor, {key,purpose:"settle-existing"}, tx => nativeExpensePayments(tx).createExpensePayment(actor, id, expectedVersion, key));
}
export function paymentGuards(
  db: SqlDatabase,
  actor: WorkContext,
  key: string,
  row: ExpenseRow,
) {
  const now = Date.now();
  return [
    staffGuard(db, key, actor.companyId, actor.userId),
    assertion(
      db,
      key,
      "payment-stage",
      `EXISTS(SELECT 1 FROM finance_expenses x WHERE id=? AND company_id=? AND workflow_stage='payment' AND status='posted' AND cancelled_at IS NULL AND (branch_id IS NULL OR EXISTS(SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=x.branch_id AND company_id=x.company_id AND status='active')) AND (department_id IS NULL OR EXISTS(SELECT 1 FROM (${organizationProjectionSources.organization_nodes}) organization_nodes WHERE id=x.department_id AND company_id=x.company_id AND status='active')))`,
      [row.id, actor.companyId],
    ),
    assertion(
      db,
      key,
      "treasury-authority",
      `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) assignments WHERE id=? AND company_id=? AND iam_role_allows(role,company_id,'expense.pay') AND revoked_at IS NULL AND iam_assignment_active(id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(user_id,company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?) OR (scope_kind='department' AND scope_node_id=?)))`,
      [
        actor.assignmentId,
        actor.companyId,
        now,
        now,
        row.branch_id,
        row.department_id,
      ],
    ),
    assertion(
      db,
      key,
      "payment-independent",
      `EXISTS(SELECT 1 FROM finance_expenses WHERE id=? AND claimant_user_id!=? AND created_by!=? AND prepared_by!=? AND COALESCE(erpx_json_extract(accounting_json,'$.preparedBy'),'')!=?) AND NOT EXISTS(SELECT 1 FROM (${workflowProjectionSources.approval_instances}) i JOIN (${workflowProjectionSources.approval_steps}) s ON s.instance_id=i.id WHERE i.record_type='expense' AND i.record_id=? AND i.company_id=? AND s.state='approved' AND s.approver_user_id=? AND i.cycle=(SELECT MAX(cycle) FROM (${workflowProjectionSources.approval_instances}) approval_instances WHERE record_type='expense' AND record_id=?))`,
      [
        row.id,
        actor.userId,
        actor.userId,
        actor.userId,
        actor.userId,
        row.id,
        actor.companyId,
        actor.userId,
        row.id,
      ],
    ),
  ];
}
export async function confirmExpensePayment(actor: WorkContext, id: string, input: z.infer<typeof expensePaymentSchema>, key: string, expenseId?: string) {
  const { write } = await holdingResources(actor.holdingId);
  return withExpensePurchasedCommand(write, actor, {key,purpose:"settle-existing"}, tx => nativeExpensePayments(tx).confirmExpensePayment(actor, id, input, key, expenseId));
}
function originalNativeExpensePaymentPreparation(write: SqlDatabase) {
  return createSaveExpensePayment({
    ...expenseCommandPort(write),
    payment: (actor, id) => paymentFromDatabase(write, actor, id, true),
    money: decimalToMinor,
    evidence: (actor, id, version, key) => preparePaymentEvidence(write, actor, id, version, key, "expense_payment_preparation"),
    async prepareSaving(actor, key, id, input, expense, amount, capturedEvidence) {
      const evidence = capturedEvidence as Awaited<ReturnType<typeof preparePaymentEvidence>>;
      return expenseMutation({ id, version: input.expectedVersion + 1 }, [
      ...paymentGuards(write, actor, key, expense),
      ...(evidence?.guards ?? []),
      assertion(
        write,
        key,
        "payment-draft-version",
        ppRd21IdVersionStatusGuard('expense_payment_drafts', 'draft'),
        [id, input.expectedVersion],
      ),
      nativePpStatement('pp_expense_payments_stmt_1')
        .bind(input.date, amount, input.reference, Date.now(), id),
      ...(evidence?.steps ?? []),
    ]);
    },
  });
}
export async function saveExpensePayment(actor: WorkContext, id: string, input: z.infer<typeof expensePaymentSchema>, key: string, expenseId: string) {
  const { write } = await holdingResources(actor.holdingId);
  return withExpensePurchasedCommand(write, actor, {key,purpose:"settle-existing"}, tx => nativeExpensePaymentPreparation(tx)(actor, id, input, key, expenseId));
}
function originalNativeExpensePaymentSlipRemoval(write: SqlDatabase) {
  return createRevokePaymentSlip({
    ...expenseCommandPort(write),
    readOnly: attachmentsV2ReadOnly,
    payment: (actor, id) => paymentFromDatabase(write, actor, id, true),
    link: (actor, id, attachmentId) => attachmentsV2Readable()
      ? write.prepare(ppRd49AttachmentLinkPoint('expense_payment'))
        .bind(attachmentId, actor.companyId, id).first<{ id: string; state: string; set_id: string }>()
      : Promise.resolve(null),
    async prepareRemoval(actor, key, id, attachmentId, expectedVersion, expense, canonical) {
      const now = Date.now();
      return expenseMutation({ id, version: expectedVersion + 1 }, [
      ...paymentGuards(write, actor, key, expense),
      assertion(
        write,
        key,
        "payment-draft-version",
        ppRd21IdVersionStatusGuard('expense_payment_drafts', 'draft'),
        [id, expectedVersion],
      ),
      assertion(
        write,
        key,
        "slip-owner",
        ppRd50SlipOwnerGuard('expense_payment', true),
        [attachmentId, id],
      ),
      ...(canonical
        ? [
            assertion(
              write,
              key,
              "canonical-slip-draft",
              ppRd49DraftLinkGuard(),
              [canonical.id],
            ),
            await filesExpenseEffects(write).withdrawLink({at:now,actorId:actor.userId,reason:"Removed from payment preparation",linkId:canonical.id,companyId:actor.companyId}),
            await filesExpenseEffects(write).withdrawEvent({id:`attachment-event-${crypto.randomUUID()}`,companyId:actor.companyId,attachmentId:attachmentId,linkId:canonical.id,setId:canonical.set_id,actorId:actor.userId,details:JSON.stringify({ reason: "Removed from payment preparation" }),at:now}),
          ]
        : []),
      await filesExpenseEffects(write).revokeAttachment({at:now,attachmentId:attachmentId,companyId:actor.companyId}),
      nativePpStatement('pp_expense_payments_stmt_2')
        .bind(now, id),
    ]);
    },
  });
}
export async function revokePaymentSlip(actor: WorkContext, expenseId: string, id: string, attachmentId: string, expectedVersion: number, key: string) {
  const { write } = await holdingResources(actor.holdingId);
  return withExpensePurchasedCommand(write, actor, {key,purpose:"settle-existing"}, tx => nativeExpensePaymentSlipRemoval(tx)(actor, expenseId, id, attachmentId, expectedVersion, key));
}

/** Binding accepts a native transaction as well as the holding write database. */
function originalNativeExpensePayments(write: SqlDatabase) {
  return createExpensePaymentCommands({
    ...expenseCommandPort(write),
    expense: (actor, id) => expenseFromDatabase(write, actor, id),
    payers: (_actor, row) => expensePayers(write, row),
    async preparePayment(actor, key, id, input, row) {
      const expectedVersion = input.expectedVersion, draftId = `expense-payment-${key}`, now = Date.now();
      return expenseMutation({ id, draftId, version: expectedVersion }, [
      ...paymentGuards(write, actor, key, row),
      assertion(
        write,
        key,
        "payment-claim-version",
        ppRd48IdCompanyVersionPairGuard('finance_expenses', "status='posted'", "workflow_stage='payment'"),
        [id, actor.companyId, expectedVersion],
      ),
      nativePpStatement('pp_expense_payments_stmt_3')
        .bind(draftId, id, actor.companyId, actor.userId, now, now),
    ]);
    },
    payment: (actor, id) => paymentFromDatabase(write, actor, id, true),
    payable: (actor, expense) => write.prepare(ppRd48ExpenseClaimInvoice()).bind(expense.invoice_id, actor.companyId, expense.id).first<ExpenseRow>(),
    paymentEvidence: (actor, id, version, key) => preparePaymentEvidence(write, actor, id, version, key, "expense_payment_confirmation"),
    async hasPaymentSlip(id) { return Boolean(await write.prepare(`SELECT 1 WHERE ${paymentSlipExistsSql()}`).bind(id).first()); },
    async prepareConfirmation(actor, key, id, input, expense, draft, invoice, capturedEvidence) {
      const evidence = capturedEvidence as Awaited<ReturnType<typeof preparePaymentEvidence>>;
      const prepared = await prepareInvoiceSettlement(write, actor, invoice, expense, { ...input, expectedVersion: Number(invoice.version), attachmentSetIds: [] }, key);
      const now = Date.now();
      return expenseMutation({ ...prepared.result, id: String(expense.id), draftId: id, version: input.expectedVersion + 1 }, [
      ...paymentGuards(write, actor, key, expense),
      ...(evidence?.guards ?? []),
      assertion(
        write,
        key,
        "payment-draft-version",
        ppRd27IdCompanyVersionStatusLiteralGuard('expense_payment_drafts','draft'),
        [id, actor.companyId, input.expectedVersion],
      ),
      assertion(
        write,
        key,
        "payment-slip",
        evidence ? "?=?" : paymentSlipExistsSql(),
        evidence ? [id, id] : [id],
      ),
      assertion(
        write,
        key,
        "unique-payment-slip",
        attachmentsV2Readable()
          ? `NOT EXISTS(SELECT 1 FROM (${filesProjectionSources.attachments}) current
              JOIN (${filesProjectionSources.attachment_links}) current_link ON current_link.attachment_id=current.id AND current_link.state${attachmentsV2ReadOnly() ? "='bound'" : " IN ('draft','bound')"}
              JOIN (${filesProjectionSources.attachments}) used ON used.checksum_sha256=current.checksum_sha256 AND used.owner_type='expense_payment' AND used.revoked_at IS NULL AND used.source_state='available'
              JOIN (${filesProjectionSources.attachment_links}) used_link ON used_link.attachment_id=used.id AND used_link.state='bound'
              JOIN expense_payment_drafts d ON d.id=used.owner_id AND d.company_id=? AND d.status='confirmed'
              WHERE current.owner_type='expense_payment' AND current.owner_id=? AND current.revoked_at IS NULL AND current.source_state='available')`
          : `NOT EXISTS(SELECT 1 FROM (${filesProjectionSources.attachments}) current JOIN (${filesProjectionSources.attachments}) used ON used.checksum_sha256=current.checksum_sha256 AND used.owner_type='expense_payment' AND used.revoked_at IS NULL JOIN expense_payment_drafts d ON d.id=used.owner_id AND d.company_id=? AND d.status='confirmed' WHERE current.owner_type='expense_payment' AND current.owner_id=? AND current.revoked_at IS NULL)`,
        [actor.companyId, id],
      ),
      ...prepared.steps,
      ...(evidence?.steps ?? []),
      nativePpStatement('pp_expense_payments_stmt_4')
        .bind(
          prepared.result.paymentId,
          input.date,
          decimalToMinor(input.amount),
          input.reference,
          now,
          id,
        ),
      await notification(
        write,
        {
          company_id: actor.companyId,
          branch_id: expense.branch_id ? String(expense.branch_id) : null,
          department_id: expense.department_id
            ? String(expense.department_id)
            : null,
        },
        String(expense.claimant_user_id),
        "expense",
        String(expense.id),
        Number(expense.version) + 1,
        prepared.result.outstandingMinor === 0
          ? "Your expense has been paid in full"
          : "A partial expense payment was recorded",
      ),
    ]);
    },
  });
}

export function nativeExpensePaymentPreparation(database: SqlDatabase){return (...args:Parameters<ReturnType<typeof originalNativeExpensePaymentPreparation>>)=>withExpensePurchasedCommand(database,args[0],{key:args[3],purpose:"settle-existing"},transaction=>originalNativeExpensePaymentPreparation(transaction)(...args));}

export function nativeExpensePaymentSlipRemoval(database: SqlDatabase){return (...args:Parameters<ReturnType<typeof originalNativeExpensePaymentSlipRemoval>>)=>withExpensePurchasedCommand(database,args[0],{key:args[5],purpose:"settle-existing"},transaction=>originalNativeExpensePaymentSlipRemoval(transaction)(...args));}

export function nativeExpensePayments(database:SqlDatabase){return {
 createExpensePayment:(...args:Parameters<ReturnType<typeof originalNativeExpensePayments>["createExpensePayment"]>)=>withExpensePurchasedCommand(database,args[0],{key:args[3],purpose:"settle-existing"},transaction=>originalNativeExpensePayments(transaction).createExpensePayment(...args)),
 confirmExpensePayment:(...args:Parameters<ReturnType<typeof originalNativeExpensePayments>["confirmExpensePayment"]>)=>withExpensePurchasedCommand(database,args[0],{key:args[3],purpose:"settle-existing"},transaction=>originalNativeExpensePayments(transaction).confirmExpensePayment(...args))
};}
