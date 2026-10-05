import { z } from "zod";
import type { WorkContext } from "./authorization";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { executeCommand, replayCommand, assertion } from "./commands";
import { ApiError } from "./http";
import { samePerson } from "@/lib/iam";
import { sha256 } from "./crypto";
import { minorToDecimal } from "@/lib/finance-money";
import { invoiceHistoryCte } from "./finance-history";
import { financeReviewSchema } from "./finance-books";
import { requireFinanceTool } from "./finance-company-tools";
import { readListPage } from "./list-page";
export const collectionCaseSchema = z.object({
  invoiceId: z.string().min(1),
  expectedVersion: z.number().int().positive(),
  asOf: z.string().date(),
  message: z.string().trim().min(3).max(4000).optional(),
});
export const reviseCollectionCaseSchema = collectionCaseSchema
  .omit({ invoiceId: true })
  .extend({
    expectedInvoiceVersion: z.number().int().positive(),
    reason: z.string().trim().min(3).max(2000),
  });
type Invoice = {
  id: string;
  version: number;
  kind: string;
  status: string;
  outstanding_minor: number;
  currency: string;
  due_date: string;
  invoice_number: string;
  party_id: string;
  party_name: string;
  party_version: number;
};
function stage(days: number) {
  return days > 90
    ? "over_90"
    : days > 60
      ? "61_90"
      : days > 30
        ? "31_60"
        : "1_30";
}
async function collectionSource(actor: WorkContext, id: string) {
  const { write } = await holdingResources(actor.holdingId);
  const row = await write
    .prepare(
      "SELECT i.*,p.name party_name,p.version party_version FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id WHERE i.id=? AND i.company_id=? AND i.kind='receivable' AND i.status IN ('posted','partially_paid') AND i.outstanding_minor>0",
    )
    .bind(id, actor.companyId)
    .first<Invoice>();
  if (!row)
    throw new ApiError(
      404,
      "COLLECTION_INVOICE_NOT_FOUND",
      "Select an open Company customer invoice.",
    );
  return row;
}
export async function prepareCollectionCase(
  actor: WorkContext,
  input: z.infer<typeof collectionCaseSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  input = collectionCaseSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId),
    op = "finance.collection.prepare",
    receipt = await write
      .prepare("SELECT record_id FROM command_receipts WHERE command_id=?")
      .bind(key)
      .first<{ record_id: string }>();
  if (receipt) {
    const prior = await replayCommand(
      write,
      actor,
      key,
      op,
      receipt.record_id,
      input,
    );
    if (prior) return prior;
  }
  await requireFinanceTool(write, actor.companyId, "collections");
  const row = await collectionSource(actor, input.invoiceId);
  if (row.version !== input.expectedVersion)
    throw new ApiError(
      409,
      "STALE_VERSION",
      "Reload the customer obligation before preparing a collection case.",
    );
  const days = Math.floor(
    (Date.parse(input.asOf) - Date.parse(row.due_date)) / 86400000,
  );
  if (days <= 0)
    throw new ApiError(
      422,
      "INVOICE_NOT_OVERDUE",
      "Collection cases require an overdue invoice.",
    );
  const ageStage = stage(days),
    id = crypto.randomUUID(),
    hash = await sha256(
      JSON.stringify({
        version: row.version,
        amount: row.outstanding_minor,
        status: row.status,
        partyVersion: row.party_version,
      }),
    ),
    message =
      input.message ??
      `${row.party_name}: invoice ${row.invoice_number}, ${row.currency} ${minorToDecimal(row.outstanding_minor)}, was due ${row.due_date} (${days} days overdue). Please check payment status and contact us to arrange settlement.`;
  if (
    await write
      .prepare(
        "SELECT 1 FROM finance_collection_cases WHERE company_id=? AND invoice_id=? AND invoice_version=? AND stage=?",
      )
      .bind(actor.companyId, row.id, row.version, ageStage)
      .first()
  )
    throw new ApiError(
      409,
      "COLLECTION_CASE_ALREADY_PREPARED",
      "Open the existing collection case and revise its unsent message.",
    );
  return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    1,
    { id, status: "draft", version: 1, message, stage: ageStage },
    [
      assertion(
        write,
        key,
        "tool",
        "EXISTS(SELECT 1 FROM finance_company_tools WHERE company_id=? AND tool='collections' AND enabled=1)",
        [actor.companyId],
      ),
      assertion(
        write,
        key,
        "invoice",
        "EXISTS(SELECT 1 FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id WHERE i.id=? AND i.company_id=? AND i.version=? AND i.outstanding_minor=? AND i.status IN ('posted','partially_paid') AND p.version=?)",
        [
          row.id,
          actor.companyId,
          row.version,
          row.outstanding_minor,
          row.party_version,
        ],
      ),
      write
        .prepare(
          "INSERT INTO finance_collection_cases(id,company_id,invoice_id,invoice_version,as_of_date,stage,amount_minor,currency,message,source_hash,prepared_by,created_at,root_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          row.id,
          row.version,
          input.asOf,
          ageStage,
          row.outstanding_minor,
          row.currency,
          message,
          hash,
          actor.userId,
          Date.now(),
          id,
        ),
    ],
  );
}
export async function reviseCollectionCase(
  actor: WorkContext,
  parentId: string,
  input: z.infer<typeof reviseCollectionCaseSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  input = reviseCollectionCaseSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId),
    op = "finance.collection.revise";
  const receipt = await write
    .prepare("SELECT record_id FROM command_receipts WHERE command_id=?")
    .bind(key)
    .first<{ record_id: string }>();
  if (receipt) {
    const prior = await replayCommand(
      write,
      actor,
      key,
      op,
      receipt.record_id,
      { parentId, ...input },
    );
    if (prior) return prior;
  }
  const parent = await write
    .prepare(
      "SELECT * FROM finance_collection_cases WHERE id=? AND company_id=?",
    )
    .bind(parentId, actor.companyId)
    .first<{
      id: string;
      root_id: string;
      revision_number: number;
      version: number;
      invoice_id: string;
      stage: string;
      status: string;
      as_of_date: string;
    }>();
  if (!parent)
    throw new ApiError(
      404,
      "COLLECTION_CASE_NOT_FOUND",
      "Collection case not found.",
    );
  await requireFinanceTool(write, actor.companyId, "collections");
  if (
    parent.version !== input.expectedVersion ||
    !["draft", "approved", "cancelled"].includes(parent.status) ||
    (await write
      .prepare("SELECT 1 FROM finance_collection_cases WHERE parent_case_id=?")
      .bind(parentId)
      .first())
  )
    throw new ApiError(
      409,
      "COLLECTION_EDITION_CHANGED",
      "Open the current unsent collection edition before revising it.",
    );
  if (
    await write
      .prepare(
        "SELECT 1 FROM finance_collection_deliveries d JOIN finance_collection_cases c ON c.id=d.case_id WHERE c.root_id=?",
      )
      .bind(parent.root_id)
      .first()
  )
    throw new ApiError(
      409,
      "COLLECTION_ALREADY_DELIVERED",
      "Delivered text is retained. A later invoice version or aging stage requires its own reviewed case.",
    );
  const invoice = await collectionSource(actor, parent.invoice_id);
  if (invoice.version !== input.expectedInvoiceVersion)
    throw new ApiError(
      409,
      "STALE_VERSION",
      "Reload the current customer obligation before revising its message.",
    );
  const days = Math.floor(
    (Date.parse(input.asOf) - Date.parse(invoice.due_date)) / 86400000,
  );
  if (
    input.asOf < parent.as_of_date ||
    days <= 0 ||
    stage(days) !== parent.stage
  )
    throw new ApiError(
      422,
      "COLLECTION_STAGE_CHANGED",
      "Use the original aging stage and a current date, or prepare the next stage as a new case.",
    );
  const id = crypto.randomUUID(),
    now = Date.now();
  const sourceHash = await sha256(
    JSON.stringify({
      version: invoice.version,
      amount: invoice.outstanding_minor,
      status: invoice.status,
      partyVersion: invoice.party_version,
    }),
  );
  const message =
    input.message ??
    `${invoice.party_name}: invoice ${invoice.invoice_number}, ${invoice.currency} ${minorToDecimal(invoice.outstanding_minor)}, was due ${invoice.due_date} (${days} days overdue). Please check payment status and contact us to arrange settlement.`;
  return executeCommand(
    write,
    actor,
    key,
    op,
    { parentId, ...input },
    id,
    1,
    {
      id,
      rootId: parent.root_id,
      parentId,
      revision: parent.revision_number + 1,
      version: 1,
      status: "draft",
      message,
      stage: parent.stage,
    },
    [
      assertion(
        write,
        key,
        "tool",
        "EXISTS(SELECT 1 FROM finance_company_tools WHERE company_id=? AND tool='collections' AND enabled=1)",
        [actor.companyId],
      ),
      assertion(
        write,
        key,
        "parent",
        "EXISTS(SELECT 1 FROM finance_collection_cases WHERE id=? AND company_id=? AND version=? AND status=?) AND NOT EXISTS(SELECT 1 FROM finance_collection_cases WHERE parent_case_id=?) AND NOT EXISTS(SELECT 1 FROM finance_collection_deliveries d JOIN finance_collection_cases c ON c.id=d.case_id WHERE c.root_id=?)",
        [
          parentId,
          actor.companyId,
          input.expectedVersion,
          parent.status,
          parentId,
          parent.root_id,
        ],
      ),
      assertion(
        write,
        key,
        "invoice",
        "EXISTS(SELECT 1 FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id WHERE i.id=? AND i.company_id=? AND i.version=? AND i.outstanding_minor=? AND i.status=? AND p.version=?)",
        [
          invoice.id,
          actor.companyId,
          invoice.version,
          invoice.outstanding_minor,
          invoice.status,
          invoice.party_version,
        ],
      ),
      write
        .prepare(
          "UPDATE finance_collection_cases SET status='superseded',version=version+1 WHERE id=?",
        )
        .bind(parentId),
      write
        .prepare(
          "INSERT INTO finance_collection_cases(id,company_id,invoice_id,invoice_version,as_of_date,stage,amount_minor,currency,message,source_hash,prepared_by,created_at,root_id,revision_number,parent_case_id,parent_version,revision_reason) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          invoice.id,
          invoice.version,
          input.asOf,
          parent.stage,
          invoice.outstanding_minor,
          invoice.currency,
          message,
          sourceHash,
          actor.userId,
          now,
          parent.root_id,
          parent.revision_number + 1,
          parentId,
          parent.version + 1,
          input.reason,
        ),
    ],
  );
}
export async function decideCollectionCase(
  actor: WorkContext,
  id: string,
  input: z.infer<typeof financeReviewSchema>,
  action: "approve" | "cancel" | "test-deliver",
  key: string,
) {
  actor = requirePermission(
    actor,
    action === "approve" ? "finance.approve" : "finance.write",
  );
  input = financeReviewSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId),
    op = `finance.collection.${action}`,
    prior = await replayCommand(write, actor, key, op, id, input);
  if (prior) return prior;
  const row = await write
    .prepare(
      "SELECT * FROM finance_collection_cases WHERE id=? AND company_id=?",
    )
    .bind(id, actor.companyId)
    .first<{
      status: string;
      version: number;
      invoice_id: string;
      invoice_version: number;
      source_hash: string;
      message: string;
      prepared_by: string;
      reviewed_by: string | null;
    }>();
  if (!row)
    throw new ApiError(
      404,
      "COLLECTION_CASE_NOT_FOUND",
      "Collection case not found.",
    );
  if (
    row.version !== input.expectedVersion ||
    ["cancelled", "superseded", "test_delivered"].includes(row.status) ||
    (action === "approve" && row.status !== "draft") ||
    (action === "test-deliver" && row.status !== "approved")
  )
    throw new ApiError(
      409,
      "STALE_VERSION",
      "Reload the collection case before continuing.",
    );
  if (action === "approve" && samePerson(actor, row.prepared_by))
    throw new ApiError(
      403,
      "SELF_REVIEW_FORBIDDEN",
      "Another person must approve this customer message.",
    );
  const steps = [];
  if (action !== "cancel") {
    const invoice = await collectionSource(actor, row.invoice_id),
      hash = await sha256(
        JSON.stringify({
          version: invoice.version,
          amount: invoice.outstanding_minor,
          status: invoice.status,
          partyVersion: invoice.party_version,
        }),
      );
    if (hash !== row.source_hash)
      throw new ApiError(
        409,
        "STALE_COLLECTION_EVIDENCE",
        "The customer or amount changed. Prepare a fresh collection case.",
      );
    steps.push(
      assertion(
        write,
        key,
        "invoice",
        "EXISTS(SELECT 1 FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id WHERE i.id=? AND i.company_id=? AND i.version=? AND i.outstanding_minor=? AND i.status=? AND p.version=?)",
        [
          invoice.id,
          actor.companyId,
          invoice.version,
          invoice.outstanding_minor,
          invoice.status,
          invoice.party_version,
        ],
      ),
    );
  }
  const status =
      action === "approve"
        ? "approved"
        : action === "cancel"
          ? "cancelled"
          : "test_delivered",
    now = Date.now();
  steps.push(
    assertion(
      write,
      key,
      "case-version",
      "EXISTS(SELECT 1 FROM finance_collection_cases WHERE id=? AND company_id=? AND version=? AND status=?)",
      [id, actor.companyId, input.expectedVersion, row.status],
    ),
  );
  if (action === "test-deliver")
    steps.push(
      write
        .prepare(
          "INSERT INTO finance_collection_deliveries(case_id,company_id,adapter,delivered_by,payload_hash,delivered_at) VALUES(?,?,'test',?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          actor.userId,
          await sha256(row.message),
          now,
        ),
    );
  steps.push(
    write
      .prepare(
        "UPDATE finance_collection_cases SET status=?,version=version+1,reviewed_by=CASE WHEN ?='approved' THEN ? ELSE reviewed_by END,review_reason=CASE WHEN ?='approved' THEN ? ELSE review_reason END,reviewed_at=CASE WHEN ?='approved' THEN ? ELSE reviewed_at END WHERE id=?",
      )
      .bind(
        status,
        status,
        actor.userId,
        status,
        input.reason,
        status,
        now,
        id,
      ),
  );
  return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    row.version + 1,
    {
      id,
      status,
      version: row.version + 1,
      adapter: action === "test-deliver" ? "test" : null,
    },
    steps,
  );
}
export type CollectionQuery = {
  invoiceCursor?: string;
  caseCursor?: string;
  q?: string;
};
export async function collectionWorkspace(
  actor: WorkContext,
  asOf: string,
  query: CollectionQuery = {},
) {
  actor = requirePermission(actor, "finance.read");
  asOf = z.string().date().parse(asOf);
  const { write } = await holdingResources(actor.holdingId);
  const search = z
    .string()
    .max(200)
    .parse(query.q ?? "");
  const [invoices, cases] = await Promise.all([
    readListPage<Record<string, unknown> & { id: string; page_key: string }>(
      write,
      {
        sql: "SELECT i.id,i.invoice_number,i.due_date,i.currency,i.outstanding_minor,i.version,p.name party_name,i.due_date page_key FROM finance_invoices i JOIN finance_parties p ON p.id=i.party_id AND p.company_id=i.company_id WHERE i.company_id=? AND i.kind='receivable' AND i.status IN ('posted','partially_paid') AND i.outstanding_minor>0 AND i.due_date<? AND (?='' OR strpos(lower(i.invoice_number||' '||p.name),lower(?))>0)",
        args: [actor.companyId, asOf, search, search],
        cursor: query.invoiceCursor,
        scope: [actor.companyId, "finance-collection-invoices", asOf, search],
      },
    ),
    readListPage<Record<string, unknown> & { id: string; page_key: string }>(
      write,
      {
        sql: "SELECT c.*,i.version current_invoice_version,CASE WHEN EXISTS(SELECT 1 FROM finance_collection_cases child WHERE child.parent_case_id=c.id) THEN 0 ELSE 1 END is_current,lpad(c.created_at::text,20,'0') page_key FROM finance_collection_cases c JOIN finance_invoices i ON i.id=c.invoice_id AND i.company_id=c.company_id JOIN finance_parties p ON p.id=i.party_id AND p.company_id=c.company_id WHERE c.company_id=? AND (?='' OR strpos(lower(c.message||' '||i.invoice_number||' '||p.name),lower(?))>0)",
        args: [actor.companyId, search, search],
        cursor: query.caseCursor,
        descending: true,
        scope: [actor.companyId, "finance-collection-cases", search],
      },
    ),
  ]);
  return {
    asOf,
    invoices: invoices.items,
    cases: cases.items,
    pages: {
      invoices: {
        nextCursor: invoices.nextCursor,
        previousCursor: invoices.previousCursor,
      },
      cases: {
        nextCursor: cases.nextCursor,
        previousCursor: cases.previousCursor,
      },
    },
  };
}
export async function cashDueForecast(
  actor: WorkContext,
  input: { chartId: string; asOf: string },
) {
  actor = requirePermission(actor, "finance.read");
  input = z
    .object({ chartId: z.string().min(1), asOf: z.string().date() })
    .parse(input);
  const { write } = await holdingResources(actor.holdingId),
    chart = await write
      .prepare(
        "SELECT base_currency FROM finance_charts WHERE id=? AND company_id=?",
      )
      .bind(input.chartId, actor.companyId)
      .first<{ base_currency: string }>();
  if (!chart)
    throw new ApiError(404, "CHART_NOT_FOUND", "Company book not found.");
  const bookSource = await write
    .prepare(
      "SELECT primary_chart_id FROM finance_book_rules WHERE company_id=? AND secondary_chart_id=? AND status='approved' AND effective_date<=? ORDER BY effective_date DESC LIMIT 1",
    )
    .bind(actor.companyId, input.chartId, input.asOf)
    .first<{ primary_chart_id: string }>();
  const exposures = (
    await write
      .prepare(
        `${invoiceHistoryCte} SELECT id,kind,due_date,currency,historical_outstanding_minor,historical_base_outstanding_minor FROM historical_invoice_balances WHERE chart_id=? AND historical_base_outstanding_minor<>0 ORDER BY due_date,id`,
      )
      .bind(
        actor.companyId,
        input.asOf,
        bookSource?.primary_chart_id ?? input.chartId,
      )
      .all<{
        id: string;
        kind: string;
        due_date: string;
        currency: string;
        historical_outstanding_minor: number;
        historical_base_outstanding_minor: number;
      }>()
  ).results;
  const opening = Number(
    (await write
      .prepare(
        "SELECT COALESCE(SUM(l.base_debit_minor-l.base_credit_minor),0) value FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id JOIN finance_accounts a ON a.id=l.account_id WHERE j.company_id=? AND j.chart_id=? AND j.status IN ('posted','reversed') AND j.entry_date<=? AND a.control_kind='bank'",
      )
      .bind(actor.companyId, input.chartId, input.asOf)
      .first("value")) ?? 0,
  );
  let cash = BigInt(opening);
  const start = new Date(`${input.asOf.slice(0, 7)}-01T00:00:00Z`),
    months = Array.from({ length: 12 }, (_, i) => {
      const date = new Date(start);
      date.setUTCMonth(start.getUTCMonth() + i);
      const month = date.toISOString().slice(0, 7),
        items = exposures.filter(
          (r) =>
            (r.due_date.slice(0, 7) < input.asOf.slice(0, 7)
              ? input.asOf.slice(0, 7)
              : r.due_date.slice(0, 7)) === month,
        );
      const movement = items.reduce(
        (s, r) =>
          s +
          BigInt(r.historical_base_outstanding_minor) *
            (r.kind === "receivable" ? BigInt(1) : BigInt(-1)),
        BigInt(0),
      );
      cash += movement;
      if (
        cash > BigInt(Number.MAX_SAFE_INTEGER) ||
        cash < BigInt(Number.MIN_SAFE_INTEGER)
      )
        throw new ApiError(
          422,
          "MONEY_OVERFLOW",
          "Forecast exceeds supported precision.",
        );
      return {
        month,
        movement: minorToDecimal(Number(movement)),
        closing: minorToDecimal(Number(cash)),
        invoiceIds: items.map((r) => r.id),
      };
    });
  return {
    ...input,
    currency: chart.base_currency,
    opening: minorToDecimal(opening),
    basis:
      "Due dates and recorded functional carrying amounts; future FX and unissued business are not estimated.",
    months,
  };
}
