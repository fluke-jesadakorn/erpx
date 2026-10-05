import "server-only";
import { z } from "zod";
import {
  businessContractSchema,
  businessBillingSchema,
  contractBillingCalculation,
  type BusinessContractInput,
} from "@/lib/finance-contract-policy";
import { decimalToMinor } from "@/lib/finance-money";
import { prepareOwnedJournalReversal } from "./finance-owned-reversal";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { assertion, executeCommand, replayCommand } from "./commands";
import { ApiError } from "./http";
import { sha256 } from "./crypto";
import {
  financeTransaction,
  financeOwnedTransaction,
} from "./finance-transactions";
import { requireFinanceTool } from "./finance-company-tools";
import { requireFinanceMoneyScale } from "./finance-money-policy";
import { createInvoice, invoiceSchema, prepareJournal } from "./finance";
export { businessContractSchema, businessBillingSchema };
type Row = Record<string, string | number | null>;
type BillingResult = ReturnType<typeof contractBillingCalculation>;
export const recognitionRunSchema = z.object({
  billingRunId: z.string().min(1),
  itemKey: z.string().min(1),
  periodId: z.string().min(1),
  entryDate: z.string().date(),
  reason: z.string().trim().min(3).max(2000),
});
export const businessDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});
export const billingRevisionSchema = z
  .object({
    expectedVersion: z.number().int().positive(),
    invoiceExpectedVersion: z.number().int().positive().optional(),
    billing: businessBillingSchema.omit({ expectedVersion: true }),
    pricing: businessContractSchema.shape.pricing.optional(),
    obligations: businessContractSchema.shape.obligations.optional(),
    reason: z.string().trim().min(3).max(2000),
  })
  .strict();
type BillingRevision = z.infer<typeof billingRevisionSchema> & {
  parentId: string;
};
function billingRevisionRequest(revision: BillingRevision) {
  const { parentId, ...fields } = revision;
  return { parentId, ...billingRevisionSchema.parse(fields) };
}
function billingHashInput(
  c: Row,
  input: z.infer<typeof businessBillingSchema>,
  result: BillingResult,
  parent?: Row,
  content?: BusinessContractInput,
  reason?: string,
) {
  return {
    contractId: c.id,
    contractVersion: c.version,
    contractHash: c.source_hash,
    input,
    result,
    ...(parent
      ? {
          parentBillId: parent.id,
          parentVersion: parent.version,
          parentHash: parent.source_hash,
          terms: content,
          reason,
        }
      : {}),
  };
}
async function contract(db: SqlDatabase, actor: WorkContext, id: string) {
  const row = await db
    .prepare(
      "SELECT * FROM finance_business_contracts WHERE id=? AND company_id=?",
    )
    .bind(id, actor.companyId)
    .first<Row>();
  if (!row)
    throw new ApiError(
      404,
      "CONTRACT_NOT_FOUND",
      "Business contract not found.",
    );
  return row;
}
export async function businessContractConfiguration(
  db: SqlDatabase,
  actor: WorkContext,
  input: BusinessContractInput,
) {
  await requireFinanceMoneyScale(db, actor.companyId, input.currency);
  const chart = await db
    .prepare(
      "SELECT id,base_currency,status FROM finance_charts WHERE id=? AND company_id=?",
    )
    .bind(input.chartId, actor.companyId)
    .first<Row>();
  if (
    !chart ||
    chart.status !== "active" ||
    chart.base_currency !== input.currency
  )
    throw new ApiError(
      422,
      "CONTRACT_BOOK_INVALID",
      "Use an active primary book and its functional currency for business billing.",
    );
  if (
    await db
      .prepare(
        "SELECT 1 FROM finance_book_rules WHERE company_id=? AND secondary_chart_id=? AND status='approved'",
      )
      .bind(actor.companyId, input.chartId)
      .first()
  )
    throw new ApiError(
      422,
      "CONTRACT_PRIMARY_BOOK_REQUIRED",
      "Business invoices belong to the primary book.",
    );
  const ids = [
    ...new Set([
      input.receivableAccountId,
      input.deferredAccountId,
      ...input.obligations.map((o) => o.revenueAccountId),
      ...(input.taxBps ? [input.taxAccountId!] : []),
    ]),
  ];
  const accounts = (
    await db
      .prepare(
        `SELECT id,category,control_kind,currency,version FROM finance_accounts WHERE company_id=? AND chart_id=? AND status='active' AND id IN (${ids.map(() => "?").join(",")}) ORDER BY id`,
      )
      .bind(actor.companyId, input.chartId, ...ids)
      .all<Row>()
  ).results;
  const byId = new Map(accounts.map((a) => [a.id, a]));
  if (
    accounts.length !== ids.length ||
    accounts.some((a) => a.currency !== input.currency) ||
    byId.get(input.receivableAccountId)?.control_kind !== "receivable" ||
    byId.get(input.receivableAccountId)?.category !== "asset" ||
    byId.get(input.deferredAccountId)?.category !== "liability" ||
    input.obligations.some(
      (o) => byId.get(o.revenueAccountId)?.category !== "revenue",
    ) ||
    input.receivableAccountId === input.deferredAccountId ||
    input.obligations.some((o) =>
      [input.receivableAccountId, input.deferredAccountId].includes(
        o.revenueAccountId,
      ),
    ) ||
    (input.taxBps &&
      [
        input.receivableAccountId,
        input.deferredAccountId,
        ...input.obligations.map((o) => o.revenueAccountId),
      ].includes(input.taxAccountId!))
  )
    throw new ApiError(
      422,
      "CONTRACT_ACCOUNT_MAPPING_REQUIRED",
      "Map distinct receivable, deferred revenue, income and VAT accounts in this book.",
    );
  const party = await db
    .prepare(
      "SELECT id,version FROM finance_parties WHERE id=? AND company_id=? AND status='active' AND kind='customer' AND currency=? AND person_id IS NULL",
    )
    .bind(input.partyId, actor.companyId, input.currency)
    .first<Row>();
  if (!party)
    throw new ApiError(
      422,
      "CONTRACT_CUSTOMER_REQUIRED",
      "Choose an active customer with this invoice currency.",
    );
  const dims = (
    await db
      .prepare(
        "SELECT id,code,required FROM finance_dimension_definitions WHERE company_id=? AND status='active' ORDER BY code",
      )
      .bind(actor.companyId)
      .all<Row>()
  ).results;
  if (
    Object.keys(input.dimensions).some(
      (key) => !dims.some((d) => d.code === key),
    )
  )
    throw new ApiError(
      422,
      "DIMENSION_VALUE_REQUIRED",
      "Use registered active reporting dimensions for the contract.",
    );
  for (const d of dims) {
    const value = input.dimensions[String(d.code)];
    if (
      (d.required && !value) ||
      (value &&
        !(await db
          .prepare(
            "SELECT 1 FROM finance_dimension_values WHERE company_id=? AND dimension_id=? AND code=? AND status='active'",
          )
          .bind(actor.companyId, d.id, value)
          .first()))
    )
      throw new ApiError(
        422,
        "DIMENSION_VALUE_REQUIRED",
        "Complete the Company's active reporting dimensions.",
      );
  }
  let taxCode: Row | null = null;
  if (input.taxBps) {
    taxCode = await db
      .prepare(
        "SELECT id,code,rate_micros,effective_from,effective_to FROM finance_tax_codes WHERE company_id=? AND status='active' AND (code=? OR id=?) AND rate_micros=70000 AND effective_from<=? AND (effective_to IS NULL OR effective_to>=?) ORDER BY effective_from DESC LIMIT 1",
      )
      .bind(
        actor.companyId,
        input.taxCode,
        input.taxCode,
        input.startDate,
        input.startDate,
      )
      .first<Row>();
    if (!taxCode)
      throw new ApiError(
        422,
        "CONTRACT_TAX_CODE_REQUIRED",
        "Choose the active ordinary 7% Thai VAT code effective at contract commencement.",
      );
  }
  return { chart, accounts, party, taxCode, dimensions: input.dimensions };
}
export async function prepareBusinessContract(
  actor: WorkContext,
  raw: z.input<typeof businessContractSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  const input = businessContractSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    id = `business-contract-${key}`,
    op = "finance.business-contract.prepare";
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    await requireFinanceTool(db, actor.companyId, "business_contracts");
    const parent = input.parentId
      ? await contract(db, actor, input.parentId)
      : null;
    if (
      parent &&
      (parent.status !== "active" ||
        parent.version !== input.expectedVersion ||
        parent.number !== input.number ||
        parent.party_id !== input.partyId ||
        parent.chart_id !== input.chartId)
    )
      throw new ApiError(
        409,
        "CONTRACT_AMENDMENT_CHANGED",
        "Amend the current active edition while retaining its customer, number and primary book.",
      );
    if (
      parent &&
      (await db
        .prepare(
          "SELECT 1 FROM finance_business_billing_runs WHERE root_id=? AND service_to>=?",
        )
        .bind(parent.root_id, input.startDate)
        .first())
    )
      throw new ApiError(
        409,
        "CONTRACT_BILLED_WINDOW",
        "An amendment or renewal must begin after retained billed service windows. Correct existing invoices separately.",
      );
    const source = await businessContractConfiguration(db, actor, input),
      sourceHash = await sha256(JSON.stringify({ input, source })),
      now = Date.now(),
      edition = Number(parent?.edition ?? 0) + 1,
      root = String(parent?.root_id ?? id);
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      1,
      { id, status: "draft", version: 1, edition, sourceHash },
      [
        ...(parent
          ? [
              assertion(
                db,
                key,
                "contract-parent",
                "EXISTS(SELECT 1 FROM finance_business_contracts WHERE id=? AND company_id=? AND status='active' AND version=?)",
                [parent.id, actor.companyId, input.expectedVersion ?? 0],
              ),
            ]
          : []),
        db
          .prepare(
            "INSERT INTO finance_business_contracts(id,company_id,root_id,parent_id,number,name,chart_id,party_id,start_date,end_date,edition,content_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            actor.companyId,
            root,
            parent?.id ?? null,
            input.number,
            input.name,
            input.chartId,
            input.partyId,
            input.startDate,
            input.endDate,
            edition,
            JSON.stringify(input),
            sourceHash,
            actor.userId,
            input.reason,
            now,
            now,
          ),
      ],
    );
  });
}
export async function decideBusinessContract(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof businessDecisionSchema>,
  action: "approve" | "reject" | "cancel",
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  const input = businessDecisionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = `finance.business-contract.${action}`;
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const row = await contract(db, actor, id);
    if (
      row.version !== input.expectedVersion ||
      row.status !== (action === "cancel" ? "active" : "draft")
    )
      throw new ApiError(
        409,
        "CONTRACT_CHANGED",
        "Refresh the contract edition before deciding.",
      );
    if (samePerson(actor, row.prepared_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must decide this contract.",
      );
    const content = businessContractSchema.parse(
        JSON.parse(String(row.content_json)),
      ),
      parent = row.parent_id
        ? await contract(db, actor, String(row.parent_id))
        : null;
    if (action === "approve") {
      const source = await businessContractConfiguration(db, actor, content);
      if (
        (await sha256(JSON.stringify({ input: content, source }))) !==
        row.source_hash
      )
        throw new ApiError(
          409,
          "CONTRACT_SOURCE_CHANGED",
          "Contract accounts or customer evidence changed. Reject this draft and prepare a fresh edition.",
        );
      if (
        parent &&
        (parent.status !== "active" ||
          parent.version !== content.expectedVersion ||
          (await db
            .prepare(
              "SELECT 1 FROM finance_business_billing_runs WHERE root_id=? AND service_to>=?",
            )
            .bind(parent.root_id, content.startDate)
            .first()))
      )
        throw new ApiError(
          409,
          "CONTRACT_AMENDMENT_CHANGED",
          "The previous edition or its billed windows changed.",
        );
    }
    const status =
        action === "approve"
          ? "active"
          : action === "reject"
            ? "rejected"
            : "cancelled",
      version = input.expectedVersion + 1;
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      version,
      { id, status, version },
      [
        assertion(
          db,
          key,
          "contract-version",
          "EXISTS(SELECT 1 FROM finance_business_contracts WHERE id=? AND company_id=? AND status=? AND version=?)",
          [id, actor.companyId, row.status, input.expectedVersion],
        ),
        ...(action === "approve" && parent
          ? [
              db
                .prepare(
                  "UPDATE finance_business_contracts SET status='superseded',version=version+1,updated_at=? WHERE id=?",
                )
                .bind(Date.now(), parent.id),
            ]
          : []),
        db
          .prepare(
            "UPDATE finance_business_contracts SET status=?,reviewed_by=?,version=?,updated_at=? WHERE id=?",
          )
          .bind(status, actor.userId, version, Date.now(), id),
        db
          .prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)")
          .bind(
            crypto.randomUUID(),
            key,
            id,
            op,
            actor.userId,
            actor.assignmentId,
            input.expectedVersion,
            await sha256(JSON.stringify(input)),
            Date.now(),
          ),
      ],
    );
  });
}
export async function prepareBusinessBilling(
  actor: WorkContext,
  contractId: string,
  raw: z.input<typeof businessBillingSchema>,
  key: string,
  ownedTransaction?: SqlDatabase,
  revision?: BillingRevision,
  postedCorrectionId?: string,
) {
  actor = requirePermission(actor, "finance.write");
  const input = businessBillingSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    id = `business-bill-${key}`,
    op = revision
      ? "finance.business-billing.revise"
      : "finance.business-billing.prepare",
    request = revision
      ? billingRevisionRequest(revision)
      : { contractId, ...input };
  return financeOwnedTransaction(write, ownedTransaction, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, request);
    if (prior) return prior;
    await requireFinanceTool(db, actor.companyId, "business_contracts");
    const row = await contract(db, actor, contractId);
    if (
      (!revision && row.status !== "active") ||
      (revision &&
        !["active", "cancelled", "superseded"].includes(String(row.status))) ||
      row.version !== input.expectedVersion
    )
      throw new ApiError(
        409,
        "CONTRACT_CHANGED",
        "Billing requires the current independently approved contract edition.",
      );
    let parent: Row | undefined;
    const ownerCredit=postedCorrectionId?await db.prepare("SELECT * FROM finance_billing_corrections WHERE id=? AND company_id=? AND status='posted' AND reviewed_by=? AND replacement_bill_id=?").bind(postedCorrectionId,actor.companyId,actor.userId,id).first<Row>():null;
    if(postedCorrectionId&&(!ownedTransaction||!ownerCredit||!revision||ownerCredit.billing_run_id!==revision.parentId||samePerson(actor,ownerCredit.prepared_by)))throw new ApiError(409,'BILLING_CREDIT_OWNER_REQUIRED','Replacement billing requires the independent credit owner transaction.');
    if (revision) {
      parent =
        (await db
          .prepare(
            "SELECT b.*,i.status original_invoice_status,i.version original_invoice_version,i.journal_id original_journal_id,i.posted_at original_posted_at FROM finance_business_billing_runs b LEFT JOIN finance_invoices i ON i.id=b.invoice_id WHERE b.id=? AND b.company_id=? AND b.contract_id=?",
          )
          .bind(revision.parentId, actor.companyId, contractId)
          .first<Row>()) ?? undefined;
      if (
        !parent ||
        parent.version !== revision.expectedVersion ||
        (await db
          .prepare(
            "SELECT 1 FROM finance_business_billing_runs WHERE parent_run_id=?",
          )
          .bind(revision.parentId)
          .first())
      )
        throw new ApiError(
          409,
          "BILLING_REVISION_SOURCE_CHANGED",
          "Choose the current retained billing edition and refresh its version.",
        );
      if(parent.invoice_id&&parent.original_invoice_version!==revision.invoiceExpectedVersion)throw new ApiError(409,'BILLING_INVOICE_CHANGED','Refresh the current original invoice version before preparing a correction.');
      if (!ownerCredit &&
        parent.invoice_id &&
        (!["draft", "voided"].includes(
          String(parent.original_invoice_status),
        ) ||
          parent.original_journal_id ||
          parent.original_posted_at ||
          parent.original_invoice_version !== revision.invoiceExpectedVersion)
      )
        throw new ApiError(
          409,
          "BILLING_POSTED_CORRECTION_REQUIRED",
          "A posted or paid bill requires a reviewed credit/debit correction through its accounting and tax owners. This revision applies to an unposted bill.",
        );
      if (
        await db
          .prepare(
            "SELECT 1 FROM finance_recognition_runs WHERE billing_run_id=? AND status IN ('draft','posted')",
          )
          .bind(parent.id)
          .first()
      )
        throw new ApiError(
          409,
          "BILLING_POSTED_CORRECTION_REQUIRED",
          "Resolve the owned revenue recognition before preparing a billing correction.",
        );
      if (input.invoiceDate < JSON.parse(String(parent.input_json)).invoiceDate)
        throw new ApiError(
          422,
          "BILLING_REVISION_DATE_INVALID",
          "Date the corrected bill on or after its previous invoice date.",
        );
    }
    const content = businessContractSchema.parse({
      ...JSON.parse(String(parent?.contract_content_json ?? row.content_json)),
      ...(revision?.pricing ? { pricing: revision.pricing } : {}),
      ...(revision?.obligations ? { obligations: revision.obligations } : {}),
    });
    await businessContractConfiguration(db, actor, content);
    let result: BillingResult;
    try {
      result = contractBillingCalculation(content, input);
    } catch (e) {
      throw new ApiError(
        422,
        "BILLING_INPUT_INVALID",
        e instanceof Error ? e.message : "Invalid billing input.",
      );
    }
    const noCharge = decimalToMinor(result.subtotal) === 0;
    if (
      parent &&
      (result.billingKey !== parent.billing_key ||
        result.serviceFrom !== parent.service_from ||
        result.serviceTo !== parent.service_to)
    )
      throw new ApiError(
        422,
        "BILLING_REVISION_WINDOW_CHANGED",
        "Retain this original service window. Prepare a different month through the normal billing workflow.",
      );
    if (
      !parent &&
      (await db
        .prepare(
          "SELECT 1 FROM finance_business_billing_runs WHERE root_id=? AND billing_key=?",
        )
        .bind(row.root_id, result.billingKey)
        .first())
    )
      throw new ApiError(
        409,
        "BILLING_ALREADY_PREPARED",
        "This service window already has a retained billing run.",
      );
    const invoice = noCharge
      ? null
      : await createInvoice(
          actor,
          invoiceSchema.parse({
            partyId: content.partyId,
            kind: "receivable",
            invoiceNumber: parent
              ? `${content.number.slice(0, 60)}-${result.billingKey}-b${Number(parent.bill_edition) + 1}`
              : `${content.number}-${result.billingKey}-e${row.edition}`,
            invoiceDate: input.invoiceDate,
            dueDate: input.dueDate,
            currency: content.currency,
            subtotal: result.subtotal,
            tax: result.tax,
            taxCode: content.taxCode,
            taxAccountId: content.taxAccountId,
            debitAccountId: content.receivableAccountId,
            creditAccountId: content.deferredAccountId,
          }),
          `${key}-invoice`,
          undefined,
          db,
        );
    const sourceHash = await sha256(
        JSON.stringify(
          billingHashInput(
            row,
            input,
            result,
            parent,
            content,
            revision?.reason,
          ),
        ),
      ),
      now = Date.now();
    return executeCommand(
      db,
      actor,
      key,
      op,
      request,
      id,
      1,
      {
        id,
        invoiceId: invoice?.id ?? null,
        sourceHash,
        result,
        status: noCharge ? "no_charge_draft" : "draft",
      },
      [
        ...(parent
          ? [
              assertion(
                db,
                key,
                "billing-parent-version",
                "EXISTS(SELECT 1 FROM finance_business_billing_runs WHERE id=? AND company_id=? AND version=?) AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs WHERE parent_run_id=?)",
                [parent.id, actor.companyId, parent.version, parent.id],
              ),
              ...(parent.invoice_id && !ownerCredit
                ? [
                    assertion(
                      db,
                      key,
                      "billing-parent-invoice",
                      "EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND version=? AND status IN ('draft','voided') AND journal_id IS NULL AND posted_at IS NULL)",
                      [
                        parent.invoice_id,
                        actor.companyId,
                        parent.original_invoice_version,
                      ],
                    ),
                    db
                      .prepare(
                        "UPDATE finance_invoices SET status='voided',outstanding_minor=0,base_outstanding_minor=0,version=version+1,updated_at=? WHERE id=? AND company_id=?",
                      )
                      .bind(now, parent.invoice_id, actor.companyId),
                  ]
                : []),
            ]
          : []),
        assertion(
          db,
          key,
          "contract-version",
          `EXISTS(SELECT 1 FROM finance_business_contracts WHERE id=? AND company_id=? AND status ${parent ? "IN ('active','cancelled','superseded')" : "='active'"} AND version=?)`,
          [contractId, actor.companyId, row.version],
        ),
        db
          .prepare(
            "INSERT INTO finance_business_billing_runs(id,company_id,contract_id,root_id,contract_version,billing_key,service_from,service_to,input_json,result_json,source_hash,invoice_id,invoice_version,created_by,created_at,no_charge_status,parent_run_id,parent_version,bill_edition,contract_content_json,revision_reason) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            actor.companyId,
            contractId,
            row.root_id,
            row.version,
            result.billingKey,
            result.serviceFrom,
            result.serviceTo,
            JSON.stringify(input),
            JSON.stringify(result),
            sourceHash,
            invoice?.id ?? null,
            invoice?.version ?? null,
            ownerCredit?.prepared_by ?? actor.userId,
            now,
            noCharge ? "draft" : null,
            parent?.id ?? null,
            parent?.version ?? null,
            parent ? Number(parent.bill_edition) + 1 : 1,
            parent ? JSON.stringify(content) : null,
            revision?.reason ?? null,
          ),
      ],
    );
  });
}
export async function prepareBusinessBillingRevision(
  actor: WorkContext,
  parentId: string,
  raw: z.input<typeof billingRevisionSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  const input = billingRevisionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId);
  return financeTransaction(write, async (db) => {
    const previous = await replayCommand(
      db,
      actor,
      key,
      "finance.business-billing.revise",
      `business-bill-${key}`,
      { parentId, ...input },
    );
    if (previous) return previous;
    const parent = await db
      .prepare(
        "SELECT contract_id FROM finance_business_billing_runs WHERE id=? AND company_id=?",
      )
      .bind(parentId, actor.companyId)
      .first<{ contract_id: string }>();
    if (!parent)
      throw new ApiError(
        404,
        "BILLING_RUN_NOT_FOUND",
        "Billing run not found in this Company.",
      );
    const c = await contract(db, actor, parent.contract_id);
    return prepareBusinessBilling(
      actor,
      parent.contract_id,
      { ...input.billing, expectedVersion: Number(c.version) },
      key,
      db,
      { parentId, ...input },
    );
  });
}
async function recognitionSource(
  db: SqlDatabase,
  actor: WorkContext,
  billingId: string,
  itemKey: string,
) {
  const bill = await db
    .prepare(
      "SELECT b.*,COALESCE(b.contract_content_json,c.content_json) content_json FROM finance_business_billing_runs b JOIN finance_business_contracts c ON c.id=b.contract_id WHERE b.id=? AND b.company_id=?",
    )
    .bind(billingId, actor.companyId)
    .first<Row>();
  if (!bill)
    throw new ApiError(404, "BILLING_RUN_NOT_FOUND", "Billing run not found.");
  const content = businessContractSchema.parse(
      JSON.parse(String(bill.content_json)),
    ),
    result = JSON.parse(String(bill.result_json)) as BillingResult,
    item = result.items.find((i) => i.key === itemKey);
  if (!item || decimalToMinor(item.amount) <= 0)
    throw new ApiError(
      422,
      "RECOGNITION_ITEM_REQUIRED",
      "Choose a positive retained recognition item.",
    );
  if(await db.prepare("SELECT 1 FROM finance_billing_corrections WHERE billing_run_id=? AND status='posted'").bind(bill.id).first())throw new ApiError(409,'RECOGNITION_BILL_CREDITED','Use the corrected billing edition for recognition.');
  const invoice = await db
    .prepare(
      "SELECT i.id,i.status,i.line_revision,i.subtotal_minor,i.tax_minor,i.currency,i.debit_account_id,i.credit_account_id,j.id journal_id,j.version journal_version,j.status journal_status,j.entry_date posted_date FROM finance_invoices i JOIN finance_journals j ON j.id=i.journal_id WHERE i.id=? AND i.company_id=?",
    )
    .bind(bill.invoice_id, actor.companyId)
    .first<Row>();
  if (
    !invoice ||
    !["posted", "partially_paid", "paid"].includes(String(invoice.status)) ||
    invoice.journal_status !== "posted" ||
    Number(invoice.line_revision) !== 1 ||
    Number(invoice.subtotal_minor) !== decimalToMinor(result.subtotal) ||
    invoice.debit_account_id !== content.receivableAccountId ||
    invoice.credit_account_id !== content.deferredAccountId
  )
    throw new ApiError(
      409,
      "RECOGNITION_INVOICE_REQUIRED",
      "Independently post the original billed invoice before recognizing revenue.",
    );
  const config = await businessContractConfiguration(db, actor, content),
    sourceHash = await sha256(
      JSON.stringify({
        billingHash: bill.source_hash,
        item,
        invoice: { ...invoice, status: "posted" },
        config,
      }),
    );
  return { bill, content, item, invoice, sourceHash };
}
export async function prepareRecognitionRun(
  actor: WorkContext,
  raw: z.input<typeof recognitionRunSchema>,
  key: string,
  ownedTransaction?: SqlDatabase,
) {
  actor = requirePermission(actor, "finance.write");
  const input = recognitionRunSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    id = `recognition-${key}`,
    op = "finance.recognition.prepare";
  return financeOwnedTransaction(write, ownedTransaction, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    await requireFinanceTool(db, actor.companyId, "business_contracts");
    const source = await recognitionSource(
      db,
      actor,
      input.billingRunId,
      input.itemKey,
    );
    if (
      input.entryDate < source.item.date ||
      input.entryDate < String(source.invoice.posted_date)
    )
      throw new ApiError(
        422,
        "RECOGNITION_DATE_INVALID",
        "Recognition cannot precede its due date or the posted deferral.",
      );
    if (
      !(await db
        .prepare(
          "SELECT 1 FROM finance_periods WHERE id=? AND company_id=? AND state='open' AND start_date<=? AND end_date>=?",
        )
        .bind(input.periodId, actor.companyId, input.entryDate, input.entryDate)
        .first())
    )
      throw new ApiError(
        409,
        "PERIOD_NOT_OPEN",
        "Choose an open accounting period covering this recognition date.",
      );
    if (
      await db
        .prepare(
          "SELECT 1 FROM finance_recognition_runs WHERE billing_run_id=? AND item_key=? AND status IN ('draft','posted')",
        )
        .bind(input.billingRunId, input.itemKey)
        .first()
    )
      throw new ApiError(
        409,
        "RECOGNITION_ALREADY_PREPARED",
        "This recognition item already has current evidence.",
      );
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      1,
      {
        id,
        status: "draft",
        version: 1,
        amount: source.item.amount,
        sourceHash: source.sourceHash,
      },
      [
        db
          .prepare(
            "INSERT INTO finance_recognition_runs(id,company_id,billing_run_id,item_key,due_date,entry_date,period_id,amount_minor,account_id,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            actor.companyId,
            input.billingRunId,
            input.itemKey,
            source.item.date,
            input.entryDate,
            input.periodId,
            decimalToMinor(source.item.amount),
            source.item.accountId,
            source.sourceHash,
            actor.userId,
            input.reason,
            Date.now(),
            Date.now(),
          ),
      ],
    );
  });
}
export async function decideRecognitionRun(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof businessDecisionSchema>,
  action: "post" | "reject",
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  const input = businessDecisionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = `finance.recognition.${action}`;
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const row = await db
      .prepare(
        "SELECT * FROM finance_recognition_runs WHERE id=? AND company_id=?",
      )
      .bind(id, actor.companyId)
      .first<Row>();
    if (!row)
      throw new ApiError(
        404,
        "RECOGNITION_NOT_FOUND",
        "Recognition run not found.",
      );
    if (row.status !== "draft" || row.version !== input.expectedVersion)
      throw new ApiError(
        409,
        "RECOGNITION_CHANGED",
        "Refresh the draft recognition before deciding.",
      );
    if (samePerson(actor, row.prepared_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must decide recognition.",
      );
    const source =
      action === "post"
        ? await recognitionSource(
            db,
            actor,
            String(row.billing_run_id),
            String(row.item_key),
          )
        : null;
    if (source && source.sourceHash !== row.source_hash)
      throw new ApiError(
        409,
        "RECOGNITION_SOURCE_CHANGED",
        "Invoice or account evidence changed. Reject and prepare a fresh recognition run.",
      );
    const journal = source
      ? await prepareJournal(
          db,
          actor,
          {
            chartId: source.content.chartId,
            periodId: String(row.period_id),
            entryDate: String(row.entry_date),
            currency: source.content.currency,
            memo: `Revenue recognition ${source.content.number}: ${source.item.name}`,
            sourceType: "business_recognition",
            sourceId: id,
            isOpening: false,
            lines: [
              {
                accountId: source.content.deferredAccountId,
                debit: source.item.amount,
                credit: "0",
                dimensions: source.content.dimensions,
              },
              {
                accountId: source.item.accountId,
                debit: "0",
                credit: source.item.amount,
                dimensions: source.content.dimensions,
              },
            ],
          },
          key,
          "posted",
        )
      : null;
    const status = action === "post" ? "posted" : "rejected",
      version = input.expectedVersion + 1;
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      version,
      { id, status, version, journalId: journal?.result.id ?? null },
      [
        assertion(
          db,
          key,
          "recognition-version",
          "EXISTS(SELECT 1 FROM finance_recognition_runs WHERE id=? AND company_id=? AND status='draft' AND version=?)",
          [id, actor.companyId, input.expectedVersion],
        ),
        ...(journal?.steps ?? []),
        db
          .prepare(
            "UPDATE finance_recognition_runs SET status=?,version=?,reviewed_by=?,journal_id=?,reason=?,updated_at=? WHERE id=?",
          )
          .bind(
            status,
            version,
            actor.userId,
            journal?.result.id ?? null,
            input.reason,
            Date.now(),
            id,
          ),
        db
          .prepare("INSERT INTO decision_evidence VALUES(?,?,?,?,?,?,?,?,?)")
          .bind(
            crypto.randomUUID(),
            key,
            id,
            op,
            actor.userId,
            actor.assignmentId,
            input.expectedVersion,
            await sha256(JSON.stringify(input)),
            Date.now(),
          ),
      ],
    );
  });
}
export const recognitionReversalSchema = businessDecisionSchema.extend({
  periodId: z.string().min(1),
  entryDate: z.string().date(),
});
export async function reverseRecognitionRun(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof recognitionReversalSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  const input = recognitionReversalSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = "finance.recognition.reverse";
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const row = await db
      .prepare(
        "SELECT * FROM finance_recognition_runs WHERE id=? AND company_id=? AND status='posted'",
      )
      .bind(id, actor.companyId)
      .first<Row>();
    if (!row || row.version !== input.expectedVersion)
      throw new ApiError(
        409,
        "RECOGNITION_CHANGED",
        "Refresh the posted recognition before reversing it.",
      );
    if (samePerson(actor, row.prepared_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must reverse recognition.",
      );
    if (input.entryDate < String(row.entry_date))
      throw new ApiError(
        422,
        "RECOGNITION_REVERSAL_DATE",
        "Reverse on or after the original recognition date.",
      );
    const reversal = await prepareOwnedJournalReversal(
      db,
      actor,
      String(row.journal_id),
      "business_recognition",
      id,
      input,
      key,
    );
    const reversalId = reversal.id;
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      input.expectedVersion + 1,
      {
        id,
        status: "reversed",
        version: input.expectedVersion + 1,
        reversalJournalId: reversalId,
      },
      [
        assertion(
          db,
          key,
          "recognition-version",
          "EXISTS(SELECT 1 FROM finance_recognition_runs WHERE id=? AND company_id=? AND status='posted' AND version=?)",
          [id, actor.companyId, input.expectedVersion],
        ),
        ...reversal.steps,
        db
          .prepare(
            "UPDATE finance_recognition_runs SET status='reversed',version=version+1,reversal_journal_id=?,reason=?,updated_at=? WHERE id=?",
          )
          .bind(reversalId, input.reason, Date.now(), id),
      ],
    );
  });
}
export async function decideNoChargeBilling(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof businessDecisionSchema>,
  action: "approve" | "reject",
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  const input = businessDecisionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = `finance.business-zero.${action}`;
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const row = await db
      .prepare(
        "SELECT * FROM finance_business_billing_runs WHERE id=? AND company_id=? AND invoice_id IS NULL",
      )
      .bind(id, actor.companyId)
      .first<Row>();
    if (
      !row ||
      row.version !== input.expectedVersion ||
      row.no_charge_status !== "draft" ||
      (await db
        .prepare(
          "SELECT 1 FROM finance_business_billing_runs WHERE parent_run_id=?",
        )
        .bind(id)
        .first())
    )
      throw new ApiError(
        409,
        "ZERO_USAGE_BILL_CHANGED",
        "Refresh the current zero-usage edition before deciding.",
      );
    if (samePerson(actor, row.created_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must review the zero-usage evidence.",
      );
    if (action === "approve") {
      const c = await contract(db, actor, String(row.contract_id)),
        content = businessContractSchema.parse(
          JSON.parse(String(row.contract_content_json ?? c.content_json)),
        ),
        billing = businessBillingSchema.parse(
          JSON.parse(String(row.input_json)),
        ),
        result = contractBillingCalculation(content, billing);
      await businessContractConfiguration(db, actor, content);
      const parent = row.parent_run_id
        ? ((await db
            .prepare(
              "SELECT * FROM finance_business_billing_runs WHERE id=? AND company_id=?",
            )
            .bind(row.parent_run_id, actor.companyId)
            .first<Row>()) ?? undefined)
        : undefined;
      if (
        row.parent_run_id &&
        (!parent || parent.version !== row.parent_version)
      )
        throw new ApiError(
          409,
          "BILLING_REVISION_SOURCE_CHANGED",
          "The retained previous billing edition changed. Prepare a fresh correction.",
        );
      const hash = await sha256(
        JSON.stringify(
          billingHashInput(
            { ...c, version: row.contract_version },
            billing,
            result,
            parent,
            content,
            String(row.revision_reason),
          ),
        ),
      );
      if (
        (row.parent_run_id
          ? !["active", "cancelled", "superseded"].includes(String(c.status))
          : c.status !== "active") ||
        c.version !== row.contract_version ||
        hash !== row.source_hash ||
        result.subtotal !== "0.00" ||
        result.tax !== "0.00"
      )
        throw new ApiError(
          409,
          "ZERO_USAGE_EVIDENCE_CHANGED",
          "Reproduce this retained zero-usage edition and its approved contract evidence before review.",
        );
    }
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      input.expectedVersion + 1,
      {
        id,
        status: action === "approve" ? "approved" : "rejected",
        version: input.expectedVersion + 1,
      },
      [
        assertion(
          db,
          key,
          "zero-usage-version",
          "EXISTS(SELECT 1 FROM finance_business_billing_runs WHERE id=? AND company_id=? AND version=? AND invoice_id IS NULL AND no_charge_status='draft') AND NOT EXISTS(SELECT 1 FROM finance_business_billing_runs WHERE parent_run_id=?)",
          [id, actor.companyId, input.expectedVersion, id],
        ),
        db
          .prepare(
            "UPDATE finance_business_billing_runs SET no_charge_status=?,reviewed_by=?,review_reason=?,version=version+1 WHERE id=?",
          )
          .bind(
            action === "approve" ? "approved" : "rejected",
            actor.userId,
            input.reason,
            id,
          ),
      ],
    );
  });
}
export async function businessContractWorkspace(actor: WorkContext) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const [contracts, bills, recognitions, accounts, parties, charts, periods, corrections] =
    await Promise.all([
      write
        .prepare(
          "SELECT * FROM finance_business_contracts WHERE company_id=? ORDER BY created_at DESC LIMIT 200",
        )
        .bind(actor.companyId)
        .all<Row>(),
      write
        .prepare(
          "SELECT b.*,COALESCE(b.contract_content_json,c.content_json) effective_contract_content_json,COALESCE(i.status,b.no_charge_status) invoice_status,i.invoice_number,i.version current_invoice_version,CASE WHEN EXISTS(SELECT 1 FROM finance_business_billing_runs child WHERE child.parent_run_id=b.id) THEN 0 ELSE 1 END is_current FROM finance_business_billing_runs b JOIN finance_business_contracts c ON c.id=b.contract_id AND c.company_id=b.company_id LEFT JOIN finance_invoices i ON i.id=b.invoice_id WHERE b.company_id=? ORDER BY b.created_at DESC LIMIT 500",
        )
        .bind(actor.companyId)
        .all<Row>(),
      write
        .prepare(
          "SELECT * FROM finance_recognition_runs WHERE company_id=? ORDER BY created_at DESC LIMIT 500",
        )
        .bind(actor.companyId)
        .all<Row>(),
      write
        .prepare(
          "SELECT id,chart_id,code,name,category,currency,control_kind FROM finance_accounts WHERE company_id=? AND status='active' ORDER BY code",
        )
        .bind(actor.companyId)
        .all<Row>(),
      write
        .prepare(
          "SELECT id,name,currency FROM finance_parties WHERE company_id=? AND kind='customer' AND status='active' AND person_id IS NULL ORDER BY name",
        )
        .bind(actor.companyId)
        .all<Row>(),
      write
        .prepare(
          "SELECT c.id,c.name,c.base_currency FROM finance_charts c WHERE c.company_id=? AND c.status='active' AND NOT EXISTS(SELECT 1 FROM finance_book_rules r WHERE r.company_id=c.company_id AND r.secondary_chart_id=c.id AND r.status='approved') ORDER BY c.name",
        )
        .bind(actor.companyId)
        .all<Row>(),
      write
        .prepare(
          "SELECT id,name,start_date,end_date FROM finance_periods WHERE company_id=? AND state='open' ORDER BY start_date DESC",
        )
        .bind(actor.companyId)
        .all<Row>(),
      write.prepare('SELECT * FROM finance_billing_corrections WHERE company_id=? ORDER BY created_at DESC LIMIT 500').bind(actor.companyId).all<Row>(),
    ]);
  return {
    corrections:corrections.results,
    contracts: contracts.results,
    bills: bills.results,
    recognitions: recognitions.results,
    accounts: accounts.results,
    parties: parties.results,
    charts: charts.results,
    periods: periods.results,
  };
}
