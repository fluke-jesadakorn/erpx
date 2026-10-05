import { financeTransaction } from "./finance-transactions";
import { z } from "zod";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { isTransactionalSqlDatabase } from "./runtime/types";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { executeCommand, replayCommand, assertion } from "./commands";
import { ApiError } from "./http";
import { samePerson } from "@/lib/iam";
import { sha256 } from "./crypto";
import { financeCsvRows } from "@/lib/finance-csv";
import { minorToDecimal } from "@/lib/finance-money";
import { importBankStatement } from "./finance";
import { financeReviewSchema } from "./finance-books";
import { requireFinanceTool } from "./finance-company-tools";
import { readListPage } from "./list-page";
export const bankCsvMappingSchema = z.object({
  name: z.string().trim().min(1).max(160),
  columns: z.object({
    externalId: z.string().min(1),
    transactionDate: z.string().min(1),
    description: z.string().optional(),
    amount: z.string().min(1),
    currency: z.string().optional(),
  }),
  dateFormat: z.enum(["YYYY-MM-DD", "DD/MM/YYYY"]).default("YYYY-MM-DD"),
});
export const bankCsvImportSchema = z.object({
  mappingId: z.string().min(1),
  bankAccountId: z.string().min(1),
  statementNumber: z.string().trim().min(1).max(100),
  statementDate: z.string().date(),
  opening: z.string(),
  closing: z.string(),
  closingFxRate: z.string().optional(),
  csv: z.string().max(5_000_000),
});
export const bankMatchGroupSchema = z
  .object({
    bankTransactionIds: z.array(z.string().min(1)).min(1).max(100),
    journalLineIds: z.array(z.string().min(1)).min(1).max(100),
    reason: z.string().trim().min(3).max(2000),
  })
  .refine(
    (v) =>
      new Set(v.bankTransactionIds).size === v.bankTransactionIds.length &&
      new Set(v.journalLineIds).size === v.journalLineIds.length,
    "Select every bank row and journal line once.",
  );
type BankRow = {
  id: string;
  version: number;
  currency: string;
  bank_account: string;
  amount_minor: number;
  transaction_date: string;
  match_status: string;
};
type LedgerRow = {
  id: string;
  account_id: string;
  debit_minor: number;
  credit_minor: number;
  currency: string;
  status: string;
  version: number;
  entry_date: string;
  journal_id: string;
};
// These owners already retain the bank confirmation with their cash journal.
// POS can also adopt a manual journal before its own bank confirmation.
const availableCashSource = `j.source_type NOT IN ('book_projection','book_opening','withholding_remittance','withholding_remittance_reversal','payroll_remittance','payroll_payment')
  AND NOT EXISTS(SELECT 1 FROM pos_settlements s WHERE s.company_id=j.company_id AND s.journal_id=j.id)
  AND NOT EXISTS(SELECT 1 FROM finance_bank_match_targets WHERE journal_line_id=l.id)
  AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_payments p ON p.id=t.matched_id WHERE t.company_id=j.company_id AND t.match_status='matched' AND t.matched_type='payment' AND p.journal_id=j.id)
  AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.company_id=j.company_id AND t.match_status='matched' AND t.matched_type='journal' AND t.matched_id=j.id)`;
async function replay(
  db: SqlDatabase,
  actor: WorkContext,
  key: string,
  operation: string,
  input: unknown,
) {
  const r = await db
    .prepare("SELECT record_id FROM command_receipts WHERE command_id=?")
    .bind(key)
    .first<{ record_id: string }>();
  return r
    ? replayCommand<Record<string, unknown>>(
        db,
        actor,
        key,
        operation,
        r.record_id,
        input,
      )
    : null;
}
export async function createBankCsvMapping(
  actor: WorkContext,
  input: z.infer<typeof bankCsvMappingSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  input = bankCsvMappingSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId),
    op = "finance.bank-mapping.create",
    prior = await replay(write, actor, key, op, input);
  if (prior) return prior;
  await requireFinanceTool(write, actor.companyId, "bank_automation");
  const id = crypto.randomUUID();
  return executeCommand(write, actor, key, op, input, id, 1, { id }, [
    write
      .prepare(
        "INSERT INTO finance_bank_csv_mappings(id,company_id,name,columns_json,created_by,created_at) VALUES(?,?,?,?,?,?)",
      )
      .bind(
        id,
        actor.companyId,
        input.name,
        JSON.stringify(input),
        actor.userId,
        Date.now(),
      ),
  ]);
}
export async function importMappedBankCsv(
  actor: WorkContext,
  input: z.infer<typeof bankCsvImportSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  input = bankCsvImportSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId);
  if (!isTransactionalSqlDatabase(write))
    throw new ApiError(
      503,
      "TRANSACTION_REQUIRED",
      "Mapped imports require PostgreSQL.",
    );
  return financeTransaction(write, async (db) => {
    const op = "finance.bank-csv.import",
      prior = await replay(db, actor, key, op, input);
    if (prior) return prior;
    const saved = await db
      .prepare(
        "SELECT columns_json FROM finance_bank_csv_mappings WHERE id=? AND company_id=?",
      )
      .bind(input.mappingId, actor.companyId)
      .first<{ columns_json: string }>();
    if (!saved)
      throw new ApiError(
        404,
        "BANK_MAPPING_NOT_FOUND",
        "Saved CSV mapping not found.",
      );
    await requireFinanceTool(db, actor.companyId, "bank_automation");
    const mapping = bankCsvMappingSchema.parse(JSON.parse(saved.columns_json)),
      account = await db
        .prepare(
          "SELECT currency FROM finance_accounts WHERE id=? AND company_id=?",
        )
        .bind(input.bankAccountId, actor.companyId)
        .first<{ currency: string }>();
    if (!account)
      throw new ApiError(
        404,
        "BANK_ACCOUNT_NOT_FOUND",
        "Bank account not found.",
      );
    const fileHash = await sha256(
      input.csv.replace(/^\uFEFF/, "").replace(/\r\n/g, "\n"),
    );
    const imported=await db.prepare("SELECT s.statement_number FROM finance_bank_import_files f JOIN finance_bank_statements s ON s.id=f.statement_id AND s.company_id=f.company_id WHERE f.company_id=? AND f.bank_account_id=? AND f.file_hash=?").bind(actor.companyId,input.bankAccountId,fileHash).first<{statement_number:string}>();
    if(imported)throw new ApiError(409,"BANK_CSV_ALREADY_IMPORTED",`This CSV was already imported as ${imported.statement_number}. Review the existing statement before importing another file.`);
    let parsed: Record<string, string>[];
    try {
      parsed = financeCsvRows(input.csv);
    } catch (e) {
      throw new ApiError(
        422,
        "INVALID_CSV",
        e instanceof Error ? e.message : "Invalid CSV",
      );
    }
    const rows = parsed.map((r) => {
      const date = r[mapping.columns.transactionDate],
        transactionDate =
          mapping.dateFormat === "DD/MM/YYYY"
            ? date?.split("/").reverse().join("-")
            : date;
      return z
        .object({
          externalId: z.string().min(1).max(160),
          transactionDate: z.string().date(),
          description: z.string().max(2000),
          amount: z.string(),
          currency: z.string().length(3),
        })
        .parse({
          externalId: r[mapping.columns.externalId],
          transactionDate,
          description: mapping.columns.description
            ? r[mapping.columns.description]
            : "",
          amount: r[mapping.columns.amount],
          currency: mapping.columns.currency
            ? r[mapping.columns.currency]?.toUpperCase()
            : account.currency,
        });
    });
    // Explicitly pass the owned transaction to the existing import command.
    const statement = await importBankStatement(
      actor,
      {
        bankAccount: input.bankAccountId,
        statementNumber: input.statementNumber,
        statementDate: input.statementDate,
        opening: input.opening,
        closing: input.closing,
        closingFxRate: input.closingFxRate,
        sourceHash: fileHash,
        rows,
      },
      `${key}-statement`,
      db,
    );
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      String(statement.id),
      1,
      { ...statement, fileHash },
      [
        db
          .prepare(
            "INSERT INTO finance_bank_import_files(id,company_id,bank_account_id,mapping_id,file_hash,statement_id,created_at) VALUES(?,?,?,?,?,?,?)",
          )
          .bind(
            crypto.randomUUID(),
            actor.companyId,
            input.bankAccountId,
            input.mappingId,
            fileHash,
            statement.id,
            Date.now(),
          ),
      ],
    );
  });
}
async function sources(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof bankMatchGroupSchema>,
) {
  const banks: BankRow[] = [],
    lines: LedgerRow[] = [];
  for (const id of [...input.bankTransactionIds].sort()) {
    const r = await db
      .prepare(
        "SELECT t.*,s.bank_account FROM finance_bank_transactions t JOIN finance_bank_statements s ON s.id=t.statement_id WHERE t.id=? AND t.company_id=?",
      )
      .bind(id, actor.companyId)
      .first<BankRow>();
    if (
      !r ||
      r.match_status === "matched" ||
      r.match_status === "ignored" ||
      r.amount_minor === 0
    )
      throw new ApiError(
        409,
        "BANK_ROW_UNAVAILABLE",
        "Select unmatched nonzero bank movements.",
      );
    banks.push(r);
  }
  for (const id of [...input.journalLineIds].sort()) {
    const r = await db
      .prepare(
        `SELECT l.id,l.account_id,l.debit_minor,l.credit_minor,j.id journal_id,j.currency,j.status,j.version,j.entry_date FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE l.id=? AND j.company_id=? AND j.status='posted' AND j.is_opening=0 AND ${availableCashSource}`,
      )
      .bind(id, actor.companyId)
      .first<LedgerRow>();
    if (!r)
      throw new ApiError(
        409,
        "BANK_TARGET_UNAVAILABLE",
        "Select unmatched posted primary-book cash lines.",
      );
    lines.push(r);
  }
  const currency = banks[0].currency,
    account = banks[0].bank_account,
    direction = Math.sign(banks[0].amount_minor);
  if (
    banks.some(
      (r) =>
        r.currency !== currency ||
        r.bank_account !== account ||
        Math.sign(r.amount_minor) !== direction,
    ) ||
    lines.some(
      (l) =>
        l.currency !== currency ||
        l.account_id !== account ||
        Math.sign(l.debit_minor - l.credit_minor) !== direction,
    )
  )
    throw new ApiError(
      422,
      "BANK_MATCH_SCOPE",
      "Match the same bank account, currency and movement direction.",
    );
  const bankTotal = banks.reduce(
      (s, r) => s + BigInt(r.amount_minor),
      BigInt(0),
    ),
    ledgerTotal = lines.reduce(
      (s, l) => s + BigInt(l.debit_minor) - BigInt(l.credit_minor),
      BigInt(0),
    );
  if (bankTotal !== ledgerTotal)
    throw new ApiError(
      422,
      "BANK_MATCH_DIFFERENCE",
      "The selected movements and posted cash lines must total exactly the same amount.",
    );
  return {
    banks,
    lines,
    currency,
    account,
    hash: await sha256(JSON.stringify({ banks, lines })),
  };
}
export async function proposeBankMatchGroup(
  actor: WorkContext,
  input: z.infer<typeof bankMatchGroupSchema>,
  key: string,
) {
  actor = requirePermission(actor, "finance.write");
  input = bankMatchGroupSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId),
    op = "finance.bank-group.propose",
    prior = await replay(write, actor, key, op, input);
  if (prior) return prior;
  await requireFinanceTool(write, actor.companyId, "bank_automation");
  const source = await sources(write, actor, input),
    id = crypto.randomUUID(),
    now = Date.now(),
    allocations = [];
  let targetIndex = 0,
    targetRemaining = Math.abs(
      source.lines[0].debit_minor - source.lines[0].credit_minor,
    );
  for (const bank of source.banks) {
    let remaining = Math.abs(bank.amount_minor);
    while (remaining) {
      const amount = Math.min(remaining, targetRemaining),
        line = source.lines[targetIndex];
      allocations.push(
        write
          .prepare(
            "INSERT INTO finance_bank_match_allocations(id,group_id,bank_transaction_id,target_type,target_id,amount_minor,created_at) VALUES(?,?,?,'journal_line',?,?,?)",
          )
          .bind(crypto.randomUUID(), id, bank.id, line.id, amount, now),
      );
      remaining -= amount;
      targetRemaining -= amount;
      if (targetRemaining === 0 && targetIndex < source.lines.length - 1) {
        targetIndex++;
        targetRemaining = Math.abs(
          source.lines[targetIndex].debit_minor -
            source.lines[targetIndex].credit_minor,
        );
      }
    }
  }
  return executeCommand(
    write,
    actor,
    key,
    op,
    input,
    id,
    1,
    { id, status: "proposed", version: 1 },
    [
      write
        .prepare(
          "INSERT INTO finance_bank_match_groups(id,company_id,currency,status,source_hash,explanation_json,proposed_by,created_at,updated_at) VALUES(?,?,?,'proposed',?,?,?,?,?)",
        )
        .bind(
          id,
          actor.companyId,
          source.currency,
          source.hash,
          JSON.stringify({ input, source }),
          actor.userId,
          now,
          now,
        ),
      ...allocations,
    ],
  );
}
export async function decideBankMatchGroup(
  actor: WorkContext,
  id: string,
  input: z.infer<typeof financeReviewSchema>,
  action: "confirm" | "reverse" | "reject",
  key: string,
) {
  actor = requirePermission(actor, "finance.approve");
  input = financeReviewSchema.parse(input);
  const { write } = await holdingResources(actor.holdingId);
  if (!isTransactionalSqlDatabase(write))
    throw new ApiError(
      503,
      "TRANSACTION_REQUIRED",
      "Bank match approval requires PostgreSQL.",
    );
  return financeTransaction(write, async (db) => {
    const op = `finance.bank-group.${action}`,
      prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const group = await db
      .prepare(
        "SELECT * FROM finance_bank_match_groups WHERE id=? AND company_id=? FOR UPDATE",
      )
      .bind(id, actor.companyId)
      .first<{
        version: number;
        status: string;
        source_hash: string;
        explanation_json: string;
        proposed_by: string;
      }>();
    if (!group)
      throw new ApiError(
        404,
        "BANK_GROUP_NOT_FOUND",
        "Bank match group not found.",
      );
    if (
      group.version !== input.expectedVersion ||
      group.status !== (action === "reverse" ? "matched" : "proposed")
    )
      throw new ApiError(
        409,
        "STALE_VERSION",
        "Reload this match group before review.",
      );
    if (action !== "reverse" && samePerson(actor, group.proposed_by))
      throw new ApiError(
        403,
        "SELF_REVIEW_FORBIDDEN",
        "Another person must review this match group.",
      );
    const prepared = JSON.parse(group.explanation_json) as {
      input: z.infer<typeof bankMatchGroupSchema>;
    };
    if (!bankMatchGroupSchema.safeParse(prepared.input).success)
      throw new ApiError(
        409,
        "BANK_GROUP_MIGRATION_REQUIRED",
        "This legacy proposal needs a fresh source review before use.",
      );
    const now = Date.now(),
      steps = [];
    if (action === "confirm") {
      const current = await sources(db, actor, prepared.input);
      if (current.hash !== group.source_hash)
        throw new ApiError(
          409,
          "STALE_BANK_EVIDENCE",
          "Bank or journal evidence changed. Prepare a fresh proposal.",
        );
      for (const b of current.banks)
        steps.push(
          assertion(
            db,
            key,
            `bank-${b.id}`,
            "EXISTS(SELECT 1 FROM finance_bank_transactions WHERE id=? AND company_id=? AND version=? AND match_status IN ('unmatched','proposed'))",
            [b.id, actor.companyId, b.version],
          ),
          db
            .prepare(
              "UPDATE finance_bank_transactions SET match_status='matched',matched_type='group',matched_id=?,version=version+1 WHERE id=?",
            )
            .bind(id, b.id),
        );
      for (const l of current.lines)
        steps.push(
          assertion(
            db,
            key,
            `journal-${l.id}`,
            "EXISTS(SELECT 1 FROM finance_journals WHERE id=? AND company_id=? AND version=? AND status='posted')",
            [l.journal_id, actor.companyId, l.version],
          ),
          db
            .prepare(
              "INSERT INTO finance_bank_match_targets(journal_line_id,group_id) VALUES(?,?)",
            )
            .bind(l.id, id),
        );
    } else if (action === "reverse") {
      for (const b of prepared.input.bankTransactionIds)
        steps.push(
          assertion(
            db,
            key,
            `matched-${b}`,
            "EXISTS(SELECT 1 FROM finance_bank_transactions WHERE id=? AND company_id=? AND match_status='matched' AND matched_type='group' AND matched_id=?)",
            [b, actor.companyId, id],
          ),
          db
            .prepare(
              "UPDATE finance_bank_transactions SET match_status='unmatched',matched_type=NULL,matched_id=NULL,version=version+1 WHERE id=?",
            )
            .bind(b),
        );
      steps.push(
        db
          .prepare("DELETE FROM finance_bank_match_targets WHERE group_id=?")
          .bind(id),
      );
    }
    const status =
      action === "confirm"
        ? "matched"
        : action === "reverse"
          ? "reversed"
          : "rejected";
    steps.push(
      db
        .prepare(
          "UPDATE finance_bank_match_groups SET status=?,version=version+1,confirmed_by=CASE WHEN ?='matched' THEN ? ELSE confirmed_by END,reversed_by=CASE WHEN ?='reversed' THEN ? ELSE reversed_by END,updated_at=? WHERE id=?",
        )
        .bind(status, status, actor.userId, status, actor.userId, now, id),
    );
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      group.version + 1,
      { id, status, version: group.version + 1 },
      steps,
    );
  });
}
export type BankAutomationQuery = {
  bankCursor?: string;
  cashCursor?: string;
  groupCursor?: string;
  chart?: string;
  q?: string;
  cashQuery?: string;
};
export async function bankAutomationWorkspace(
  actor: WorkContext,
  query: BankAutomationQuery = {},
) {
  actor = requirePermission(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  const chart = z
    .string()
    .max(180)
    .parse(query.chart ?? "");
  const search = z
    .string()
    .max(200)
    .parse(query.q ?? "");
  const cashSearch = z
    .string()
    .max(200)
    .parse(query.cashQuery ?? "");
  const bankSql = `SELECT t.*,s.bank_account,t.transaction_date page_key FROM finance_bank_transactions t
    JOIN finance_bank_statements s ON s.id=t.statement_id AND s.company_id=t.company_id
    JOIN finance_accounts a ON a.id=s.bank_account AND a.company_id=t.company_id
    WHERE t.company_id=? AND t.match_status='unmatched' AND (?='' OR a.chart_id=?)
    AND (?='' OR strpos(lower(t.external_id||' '||COALESCE(t.description,'')),lower(?))>0)`;
  const cashSql = `SELECT l.id,l.account_id,l.description,l.debit_minor,l.credit_minor,j.id journal_id,j.journal_number,j.currency,j.entry_date,j.entry_date page_key
    FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id
    JOIN finance_accounts a ON a.id=l.account_id AND a.company_id=j.company_id
    WHERE j.company_id=? AND j.status='posted' AND a.control_kind='bank' AND j.is_opening=0
    AND (?='' OR j.chart_id=?) AND ${availableCashSource}`;
  const cashArgs = [actor.companyId, chart, chart];
  const [mappings, groups, rows, targets] = await Promise.all([
    write
      .prepare(
        "SELECT * FROM finance_bank_csv_mappings WHERE company_id=? ORDER BY name",
      )
      .bind(actor.companyId)
      .all(),
    readListPage<Record<string, unknown> & { id: string; page_key: string }>(
      write,
      {
        sql: "SELECT *,lpad(created_at::text,20,'0') page_key FROM finance_bank_match_groups WHERE company_id=?",
        args: [actor.companyId],
        descending: true,
        cursor: query.groupCursor,
        scope: [actor.companyId, "finance-bank-match-groups"],
      },
    ),
    readListPage<BankRow & { page_key: string }>(write, {
      sql: bankSql,
      args: [actor.companyId, chart, chart, search, search],
      cursor: query.bankCursor,
      scope: [actor.companyId, "finance-bank-unmatched", chart, search],
    }),
    readListPage<LedgerRow & { page_key: string }>(write, {
      sql: `SELECT * FROM (${cashSql}) cash WHERE (?='' OR strpos(lower(journal_number||' '||COALESCE(description,'')),lower(?))>0)`,
      args: [...cashArgs, cashSearch, cashSearch],
      descending: true,
      cursor: query.cashCursor,
      scope: [actor.companyId, "finance-bank-cash-lines", chart, cashSearch],
    }),
  ]);
  // Aggregate every eligible source, independently of the cash picker page.
  // Otherwise a second exact match beyond that page falsely appears unique.
  const candidates = rows.items.length
    ? (
        await write
          .prepare(
            `WITH cash AS (${cashSql})
    SELECT b.id,COUNT(c.id) FILTER(WHERE c.debit_minor-c.credit_minor=b.amount_minor) exact_count,
    MIN(c.id) FILTER(WHERE c.debit_minor-c.credit_minor=b.amount_minor) exact_id,
    COUNT(c.id) FILTER(WHERE c.entry_date=b.transaction_date) same_day_count,
    COALESCE(SUM(c.debit_minor-c.credit_minor) FILTER(WHERE c.entry_date=b.transaction_date),0)::text same_day_amount,
    CASE WHEN COUNT(c.id) FILTER(WHERE c.entry_date=b.transaction_date)<=100
      THEN COALESCE(jsonb_agg(c.id ORDER BY c.id) FILTER(WHERE c.entry_date=b.transaction_date),'[]'::jsonb)::text
      ELSE '[]' END same_day_ids
    FROM finance_bank_transactions b JOIN finance_bank_statements s ON s.id=b.statement_id AND s.company_id=b.company_id
    LEFT JOIN cash c ON c.account_id=s.bank_account AND c.currency=b.currency
      AND c.entry_date::date BETWEEN b.transaction_date::date-7 AND b.transaction_date::date+7
      AND sign(c.debit_minor-c.credit_minor)=sign(b.amount_minor)
    WHERE b.company_id=? AND b.id IN (${rows.items.map(() => "?").join(",")}) GROUP BY b.id`,
          )
          .bind(
            ...cashArgs,
            actor.companyId,
            ...rows.items.map((row) => row.id),
          )
          .all<{
            id: string;
            exact_count: number;
            exact_id: string | null;
            same_day_count: number;
            same_day_amount: string;
            same_day_ids: string;
          }>()
      ).results
    : [];
  const suggestions = rows.items.map((b) => {
    const candidate = candidates.find((row) => row.id === b.id);
    if (candidate?.exact_count === 1 && candidate.exact_id)
      return {
        bankTransactionIds: [b.id],
        journalLineIds: [candidate.exact_id],
        confidence: "amount_date",
        amount: minorToDecimal(b.amount_minor),
      };
    if (
      candidate &&
      candidate.same_day_count > 1 &&
      candidate.same_day_count <= 100 &&
      BigInt(candidate.same_day_amount) === BigInt(b.amount_minor)
    )
      return {
        bankTransactionIds: [b.id],
        journalLineIds: JSON.parse(candidate.same_day_ids) as string[],
        confidence: "same_day_total",
        amount: minorToDecimal(b.amount_minor),
      };
    return {
      bankTransactionIds: [b.id],
      journalLineIds: [],
      confidence: "needs_review",
      amount: minorToDecimal(b.amount_minor),
    };
  });
  const suggestedIds = [
    ...new Set(suggestions.flatMap((row) => row.journalLineIds)),
  ];
  const suggestionTargets = suggestedIds.length
    ? (
        await write
          .prepare(
            `SELECT * FROM (${cashSql}) cash WHERE id IN (${suggestedIds.map(() => "?").join(",")})`,
          )
          .bind(...cashArgs, ...suggestedIds)
          .all<LedgerRow>()
      ).results
    : [];
  return {
    mappings: mappings.results,
    groups: groups.items,
    suggestions,
    suggestionTargets,
    rows: rows.items,
    targets: targets.items,
    pages: {
      bank: {
        nextCursor: rows.nextCursor,
        previousCursor: rows.previousCursor,
      },
      cash: {
        nextCursor: targets.nextCursor,
        previousCursor: targets.previousCursor,
      },
      groups: {
        nextCursor: groups.nextCursor,
        previousCursor: groups.previousCursor,
      },
    },
  };
}
