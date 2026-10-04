import { InlineNotice } from "@/components/ui/foundation";
import { Table } from "@/components/ui/foundation";
import type { WorkContext } from "@/server/authorization";
import type { ExpenseRow } from "@/server/expenses";
import { holdingResources } from "@/server/runtime";
import { minorToDecimal } from "@/lib/finance-money";
import { ExpensePosting } from "./expense-posting";
import Link from "next/link";
import { Card, TableFrame } from "@/components/ui/foundation";
export async function ExpensePostingPreview({
  actor,
  expense,
  base,
}: {
  actor: WorkContext;
  expense: ExpenseRow;
  base: string;
}) {
  const { write } = await holdingResources(actor.holdingId);
  if (expense.journal_id) {
    const posted = (
      await write
        .prepare(
          "SELECT a.code,a.name,l.debit_minor,l.credit_minor FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id JOIN finance_accounts a ON a.id=l.account_id WHERE j.company_id=? AND j.id=? ORDER BY l.ordinal",
        )
        .bind(actor.companyId, expense.journal_id)
        .all<ExpenseRow>()
    ).results;
    return (
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Posted GL · Debit / Credit</h2>
        <TableFrame appearance="standard" label="Records"><Table className="business-table">
          <thead>
            <tr>
              <th>Account</th>
              <th>Debit</th>
              <th>Credit</th>
            </tr>
          </thead>
          <tbody>
            {posted.map((l, i) => (
              <tr key={i}>
                <td>
                  {String(l.code)} · {String(l.name)}
                </td>
                <td>{minorToDecimal(Number(l.debit_minor))}</td>
                <td>{minorToDecimal(Number(l.credit_minor))}</td>
              </tr>
            ))}
          </tbody>
        </Table></TableFrame>
        <Link href={`${base}/finance/journals/${expense.journal_id}`}>Open posted journal</Link>
      </Card>
    );
  }
  const config = await write
    .prepare("SELECT * FROM commerce_settings WHERE company_id=?")
    .bind(actor.companyId)
    .first<ExpenseRow>();
  const accounting = JSON.parse(String(expense.accounting_json ?? "{}")) as {
    periodId?: string;
    entryDate?: string;
  };
  if (!config)
    return (
      <p className="notice">
        Configure the company’s expense, tax and payable accounts before posting GL.
      </p>
    );
  const mappings = [
    { id: expense.category_account_id, debit: Number(expense.amount_minor), credit: 0 },
    { id: config.input_tax_account_id, debit: Number(expense.tax_minor), credit: 0 },
    {
      id: config.payable_account_id,
      debit: 0,
      credit: Number(expense.amount_minor) + Number(expense.tax_minor),
    },
  ].filter((l) => l.debit || l.credit);
  const accounts = (
    await write
      .prepare(
        "SELECT id,code,name,status,chart_id,currency FROM finance_accounts WHERE company_id=?",
      )
      .bind(actor.companyId)
      .all<ExpenseRow>()
  ).results;
  const invalid = mappings.some(
    (l) =>
      !accounts.some(
        (a) =>
          a.id === l.id &&
          a.status === "active" &&
          a.chart_id === config.chart_id &&
          a.currency === expense.currency,
      ),
  );
  const total = Number(expense.amount_minor) + Number(expense.tax_minor);
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>GL · Debit / Credit</h2>
      <p>
        {String(expense.currency)} · Posting date: {accounting.entryDate ?? "Not prepared"}
      </p>
      {invalid && (
        <InlineNotice tone="info" className="notice" role="status">
          An account mapping is missing, inactive or has a different chart/currency. Correct it
          before posting.
        </InlineNotice>
      )}
      <TableFrame appearance="standard" label="Records"><Table className="business-table">
        <thead>
          <tr>
            <th>Account</th>
            <th>Debit</th>
            <th>Credit</th>
          </tr>
        </thead>
        <tbody>
          {mappings.map((l, i) => {
            const a = accounts.find((a) => a.id === l.id);
            return (
              <tr key={i}>
                <td>{a ? `${a.code} · ${a.name}` : "Missing account mapping"}</td>
                <td>{minorToDecimal(l.debit)}</td>
                <td>{minorToDecimal(l.credit)}</td>
              </tr>
            );
          })}
        </tbody>
        <tfoot>
          <tr>
            <th>Total</th>
            <td>{minorToDecimal(total)}</td>
            <td>{minorToDecimal(total)}</td>
          </tr>
        </tfoot>
      </Table></TableFrame>
      {expense.journal_id ? (
        <Link href={`${base}/finance/journals/${expense.journal_id}`}>Open posted journal</Link>
      ) : (
        <ExpensePosting
          base={base}
          id={String(expense.id)}
          version={Number(expense.version)}
          periodId={accounting.periodId ?? ""}
          entryDate={accounting.entryDate ?? ""}
          canPost={expense.can_post === 1 && !invalid}
        />
      )}
    </Card>
  );
}
