import "server-only";
import { z } from "zod";
import { decimalToUnits } from "@/lib/finance-money";
import { samePerson } from "@/lib/iam";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";
import { holdingResources } from "./runtime";
import { requirePermission } from "./iam-permissions";
import { financeTransaction } from "./finance-transactions";
import { requireFinanceTool } from "./finance-company-tools";
import { requireFinanceMoneyScale } from "./finance-money-policy";
import { executeCommand, replayCommand, assertion } from "./commands";
import { sha256 } from "./crypto";
import { ApiError } from "./http";
type Row = Record<string, string | number | null>;
const side = z.object({
  companyId: z.string().min(1),
  journalId: z.string().min(1),
  accountId: z.string().min(1),
});
export const intercompanyPairSchema = z
  .object({
    reference: z.string().trim().min(2).max(200),
    source: side,
    target: side,
    currency: z
      .string()
      .length(3)
      .transform((v) => v.toUpperCase()),
    amount: z
      .string()
      .regex(/^\d+(\.\d{1,2})?$/)
      .refine((v) => decimalToUnits(v) > BigInt(0)),
    reason: z.string().trim().min(3).max(2000),
  })
  .refine((v) => v.source.companyId !== v.target.companyId, {
    message: "Select two different Companies.",
  });
export const intercompanyPairDecisionSchema = z.object({
  expectedVersion: z.number().int().positive(),
  reason: z.string().trim().min(3).max(2000),
});
function actorGuard(actor: WorkContext, permission: string) {
  actor = requirePermission(actor, permission);
  if (actor.scope.kind !== "holding")
    throw new ApiError(
      403,
      "HOLDING_FINANCE_REQUIRED",
      "Intercompany pairing requires Holding Finance authority.",
    );
  return actor;
}
export async function intercompanyPairSource(
  db: SqlDatabase,
  actor: WorkContext,
  input: z.infer<typeof intercompanyPairSchema>,
) {
  await requireFinanceMoneyScale(db, actor.companyId, input.currency);
  const evidence = [];
  for (const side of [input.source, input.target]) {
    const entity = await db
        .prepare(
          "SELECT * FROM finance_group_entities WHERE holding_id=? AND company_id=? AND status='active'",
        )
        .bind(actor.holdingId, side.companyId)
        .first<Row>(),
      journal = await db
        .prepare(
          "SELECT * FROM finance_journals WHERE id=? AND company_id=? AND status='posted' AND currency=?",
        )
        .bind(side.journalId, side.companyId, input.currency)
        .first<Row>();
    const lines = (
      await db
        .prepare(
          "SELECT l.*,a.code,a.category,a.version account_version FROM finance_journal_lines l JOIN finance_accounts a ON a.id=l.account_id WHERE l.journal_id=? AND l.account_id=? AND a.company_id=? ORDER BY l.ordinal",
        )
        .bind(side.journalId, side.accountId, side.companyId)
        .all<Row>()
    ).results;
    if (!entity || !journal || !lines.length)
      throw new ApiError(
        409,
        "INTERCOMPANY_POSTED_DOCUMENT_REQUIRED",
        "Select original posted document accounts from two active Group Companies in the same transaction currency.",
      );
    const net = lines.reduce(
      (s, l) => s + BigInt(l.debit_minor!) - BigInt(l.credit_minor!),
      BigInt(0),
    );
    if ((net < BigInt(0) ? -net : net) !== decimalToUnits(input.amount))
      throw new ApiError(
        409,
        "INTERCOMPANY_DOCUMENT_AMOUNT_CHANGED",
        "Pair the complete signed account amount from each posted document.",
      );
    evidence.push({ entity, journal, lines, net: String(net) });
  }
  if (BigInt(evidence[0].net) + BigInt(evidence[1].net) !== BigInt(0))
    throw new ApiError(
      422,
      "INTERCOMPANY_DOCUMENT_DIRECTIONS",
      "The two posted document accounts must have equal opposite signed amounts.",
    );
  const categories = evidence.map((e) => String(e.lines[0].category)),
    balance = categories.every((c) => ["asset", "liability"].includes(c)),
    profit = categories.every((c) =>
      ["revenue", "expense", "other_income", "other_expense"].includes(c),
    );
  if (!balance && !profit)
    throw new ApiError(
      422,
      "INTERCOMPANY_DOCUMENT_CATEGORIES",
      "Pair balance-sheet balances or income and expense documents within the same statement.",
    );
  const hash = await sha256(JSON.stringify({ input, evidence }));
  return { evidence, hash };
}
export async function prepareIntercompanyPair(
  actor: WorkContext,
  raw: z.input<typeof intercompanyPairSchema>,
  key: string,
) {
  actor = actorGuard(actor, "finance.write");
  const input = intercompanyPairSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    id = `intercompany-pair-${key}`,
    op = "finance.intercompany-pair.prepare";
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    await requireFinanceTool(db, actor.companyId, "group_reporting");
    const e = await intercompanyPairSource(db, actor, input),
      now = Date.now();
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      1,
      { id, status: "draft", version: 1 },
      [
        db
          .prepare(
            "INSERT INTO finance_intercompany_document_pairs(id,holding_id,company_id,reference,source_company_id,source_journal_id,source_account_id,target_company_id,target_journal_id,target_account_id,currency,amount_minor,source_json,source_hash,prepared_by,reason,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            id,
            actor.holdingId,
            actor.companyId,
            input.reference,
            input.source.companyId,
            input.source.journalId,
            input.source.accountId,
            input.target.companyId,
            input.target.journalId,
            input.target.accountId,
            input.currency,
            String(decimalToUnits(input.amount)),
            JSON.stringify({ input, evidence: e.evidence }),
            e.hash,
            actor.userId,
            input.reason,
            now,
            now,
          ),
        ...[input.source, input.target].map((s) =>
          db
            .prepare(
              "INSERT INTO finance_intercompany_pair_claims(pair_id,journal_id,account_id) VALUES(?,?,?)",
            )
            .bind(id, s.journalId, s.accountId),
        ),
      ],
    );
  });
}
export async function decideIntercompanyPair(
  actor: WorkContext,
  id: string,
  raw: z.input<typeof intercompanyPairDecisionSchema>,
  action: "confirm" | "reject" | "reverse",
  key: string,
) {
  actor = actorGuard(actor, "finance.approve");
  const input = intercompanyPairDecisionSchema.parse(raw),
    { write } = await holdingResources(actor.holdingId),
    op = `finance.intercompany-pair.${action}`;
  return financeTransaction(write, async (db) => {
    const prior = await replayCommand(db, actor, key, op, id, input);
    if (prior) return prior;
    const r = await db
      .prepare(
        "SELECT * FROM finance_intercompany_document_pairs WHERE id=? AND holding_id=? AND company_id=?",
      )
      .bind(id, actor.holdingId, actor.companyId)
      .first<Row>();
    if (!r)
      throw new ApiError(
        404,
        "INTERCOMPANY_PAIR_NOT_FOUND",
        "Intercompany document pair was not found.",
      );
    const expected = action === "reverse" ? "confirmed" : "draft";
    if (r.status !== expected || r.version !== input.expectedVersion)
      throw new ApiError(
        409,
        "INTERCOMPANY_PAIR_CHANGED",
        "Refresh the paired document evidence before deciding.",
      );
    if (samePerson(actor, r.prepared_by))
      throw new ApiError(
        409,
        "SELF_REVIEW_FORBIDDEN",
        "Another Finance approver must confirm or reverse this document pair.",
      );
    const definition = intercompanyPairSchema.parse(
      JSON.parse(String(r.source_json)).input,
    );
    if (
      action === "confirm" &&
      (await intercompanyPairSource(db, actor, definition)).hash !==
        r.source_hash
    )
      throw new ApiError(
        409,
        "INTERCOMPANY_PAIR_SOURCE_CHANGED",
        "A posted document or account changed. Reject and prepare again.",
      );
    const status =
        action === "confirm"
          ? "confirmed"
          : action === "reverse"
            ? "reversed"
            : "rejected",
      steps = [
        assertion(
          db,
          key,
          "intercompany-pair-version",
          "EXISTS(SELECT 1 FROM finance_intercompany_document_pairs WHERE id=? AND holding_id=? AND company_id=? AND status=? AND version=?)",
          [
            id,
            actor.holdingId,
            actor.companyId,
            expected,
            input.expectedVersion,
          ],
        ),
        db
          .prepare(
            "UPDATE finance_intercompany_document_pairs SET status=?,reviewed_by=?,version=version+1,updated_at=? WHERE id=?",
          )
          .bind(status, actor.userId, Date.now(), id),
      ];
    if (action !== "confirm")
      steps.push(
        db
          .prepare(
            "DELETE FROM finance_intercompany_pair_claims WHERE pair_id=?",
          )
          .bind(id),
      );
    return executeCommand(
      db,
      actor,
      key,
      op,
      input,
      id,
      input.expectedVersion + 1,
      { id, status, version: input.expectedVersion + 1 },
      steps,
    );
  });
}
export async function intercompanyPairWorkspace(actor: WorkContext) {
  actor = actorGuard(actor, "finance.read");
  const { write } = await holdingResources(actor.holdingId);
  return (
    await write
      .prepare(
        "SELECT * FROM finance_intercompany_document_pairs WHERE holding_id=? AND company_id=? ORDER BY created_at DESC,id LIMIT 200",
      )
      .bind(actor.holdingId, actor.companyId)
      .all<Row>()
  ).results;
}
