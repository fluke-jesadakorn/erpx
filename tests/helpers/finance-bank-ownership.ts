import { expect } from "vitest";
import type { WorkContext } from "../../src/server/authorization";
import type { SqlDatabase } from "../../src/server/runtime/types";
import { importBankStatement } from "../../src/server/finance";
import { setFinanceCompanyTool } from "../../src/server/finance-company-tools";
import {
  bankAutomationWorkspace,
  proposeBankMatchGroup,
} from "../../src/server/finance-bank-automation";

/** Exercise actual owner-posted cash against a different, still unmatched bank row. */
export async function assertOwnedCashCannotMatchAgain(
  db: SqlDatabase,
  actor: WorkContext,
  journalIds: string[],
  bankAccountId: string,
) {
  const key = () => crypto.randomUUID();
  await setFinanceCompanyTool(
    actor,
    {
      tool: "bank_automation",
      enabled: true,
      expectedVersion: 0,
      reason: "Disposable native cash ownership regression",
    },
    key(),
  );
  for (const journalId of journalIds) {
    const line = await db
      .prepare(
        "SELECT l.id,l.account_id,l.debit_minor-l.credit_minor amount,j.entry_date,j.currency FROM finance_journal_lines l JOIN finance_journals j ON j.id=l.journal_id WHERE j.id=? AND j.company_id=? AND l.account_id=?",
      )
      .bind(journalId, actor.companyId, bankAccountId)
      .first<{
        id: string;
        account_id: string;
        amount: number;
        entry_date: string;
        currency: string;
      }>();
    expect(line).not.toBeNull();
    const amount = (Number(line!.amount) / 100).toFixed(2);
    const imported = await importBankStatement(
      actor,
      {
        bankAccount: line!.account_id,
        statementNumber: key(),
        statementDate: line!.entry_date,
        opening: "0",
        closing: amount,
        sourceHash: key(),
        rows: [
          {
            externalId: key(),
            transactionDate: line!.entry_date,
            amount,
            currency: line!.currency,
          },
        ],
      },
      key(),
    );
    const bankId = String(
      await db
        .prepare(
          "SELECT id FROM finance_bank_transactions WHERE statement_id=?",
        )
        .bind(imported.id)
        .first("id"),
    );
    const workspace = await bankAutomationWorkspace(actor);
    expect(workspace.targets.map((row) => row.id)).not.toContain(line!.id);
    expect(
      workspace.suggestions.flatMap((row) => row.journalLineIds ?? []),
    ).not.toContain(line!.id);
    await expect(
      proposeBankMatchGroup(
        actor,
        {
          bankTransactionIds: [bankId],
          journalLineIds: [line!.id],
          reason: "A separate bank row cannot reuse an owned cash journal",
        },
        key(),
      ),
    ).rejects.toMatchObject({ code: "BANK_TARGET_UNAVAILABLE" });
    expect(
      await db
        .prepare(
          "SELECT match_status,version FROM finance_bank_transactions WHERE id=?",
        )
        .bind(bankId)
        .first(),
    ).toEqual({ match_status: "unmatched", version: 1 });
  }
}
