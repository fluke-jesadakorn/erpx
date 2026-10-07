import type { SqlDatabase, SqlStatement } from "../../src/server/runtime/types";

type JournalFixture = {
  id: string; companyId: string; chartId: string; periodId: string; number: string;
  date: string; currency: string; fxRateMicros: number; memo: string;
  debitAccountId: string; creditAccountId: string; amountMinor: number; branchId: string;
  status: "posted" | "reversed" | "in_review"; actorId: string; reviewerId: string; now: number;
};

/** Create one whole fictional journal; never reconcile an existing prepared journal. */
export async function seedLocalFinanceJournal(db: SqlDatabase, input: JournalFixture): Promise<boolean> {
  if (await db.prepare("SELECT 1 FROM finance_journals WHERE id=?").bind(input.id).first()) return false;
  const effects: SqlStatement[] = [
    db.prepare("INSERT INTO finance_journals(id,company_id,chart_id,period_id,journal_number,entry_date,currency,fx_rate_micros,source_type,source_id,memo,status,is_opening,version,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,'seed',?,?,'draft',0,1,?,?,?)")
      .bind(input.id, input.companyId, input.chartId, input.periodId, input.number, input.date, input.currency, input.fxRateMicros, input.id, input.memo, input.actorId, input.now, input.now),
    db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,?,'finance_journal',?,1,?,?)")
      .bind(`audit-${input.id}-create`, `seed-command-${input.id}`, input.actorId, `seed_${input.actorId}`, "finance.journal.create", input.id, "seed balanced journal", input.now),
    ...[0, 1].map(ordinal => db.prepare("INSERT INTO finance_journal_lines(id,journal_id,account_id,description,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) VALUES(?,?,?,'Operating transaction',?,?,?,?,?,?)")
      .bind(`${input.id}-${ordinal ? "cr" : "dr"}`, input.id, ordinal ? input.creditAccountId : input.debitAccountId, ordinal ? 0 : input.amountMinor, ordinal ? input.amountMinor : 0, ordinal ? 0 : input.amountMinor, ordinal ? input.amountMinor : 0, JSON.stringify({ branch: input.branchId }), ordinal)),
    db.prepare("UPDATE finance_journals SET status=?,posted_by=?,posted_at=? WHERE id=? AND status='draft'")
      .bind(input.status, input.status === "in_review" ? null : input.reviewerId, input.status === "in_review" ? null : input.now, input.id),
  ];
  if (input.status !== "in_review") effects.push(db.prepare("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at) VALUES(?,?,?,?,?,'finance_journal',?,1,?,?)")
    .bind(`audit-${input.id}-post`, `seed-command-${input.id}-post`, input.reviewerId, `seed_${input.reviewerId}`, `finance.journal.${input.status}`, input.id, "independent seed journal review", input.now));
  // One atomic batch also makes deferred immutable evidence observe both lines.
  await db.batch(effects);
  return true;
}

export async function seedLocalFinanceJournals(db: SqlDatabase, companies: ReadonlyArray<{ id: string; branches: ReadonlyArray<readonly [string, string]> }>, actorId: string, reviewerId: string, now: number) {
  for (const company of companies) for (let index = 0; index < 50; index++) {
    const currency = company.id === "beacon-co" ? "GBP" : company.id === "northstar-co" ? "EUR" : "USD";
    await seedLocalFinanceJournal(db, {
      id: `journal-${company.id}-${String(index + 1).padStart(3, "0")}`, companyId: company.id,
      chartId: `chart-${company.id}`, periodId: company.id === "acme-co" ? "period_2026_09" : `period-${company.id}-2026-09`,
      number: `JE-${company.id.toUpperCase()}-${String(index + 1).padStart(4, "0")}`,
      date: `2026-${String(7 + index % 3).padStart(2, "0")}-${String(1 + index % 27).padStart(2, "0")}`,
      currency: index % 10 === 0 ? "EUR" : currency, fxRateMicros: index % 10 === 0 ? 1_080_000 : 1_000_000,
      memo: `Seed operating entry ${index + 1}`, debitAccountId: `acct-${company.id}-${index % 2 === 0 ? "5100" : "1200"}`,
      creditAccountId: `acct-${company.id}-${index % 2 === 0 ? "1100" : "4000"}`, amountMinor: (index + 1) * 12500,
      branchId: company.branches[index % company.branches.length][0], status: index % 17 === 0 ? "reversed" : index % 11 === 0 ? "in_review" : "posted",
      actorId, reviewerId, now,
    });
  }
}
