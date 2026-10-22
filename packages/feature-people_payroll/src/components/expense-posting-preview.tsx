import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {recordCanonicalHref} from "@erpx/feature-records/lib/record-drawer";
import { InlineNotice } from "@erpx/ui/components/foundation";
import { Table } from "@erpx/ui/components/foundation";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import type { ExpenseRow } from "@erpx/server-people_payroll/expense";
import { expensePostingPreview } from "@erpx/server-people_payroll/expense";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { ExpensePosting } from "@erpx/feature-people_payroll/components/expense-posting";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { Card, TableFrame } from "@erpx/ui/components/foundation";
export async function ExpensePostingPreview({
  actor,
  expense,
  base,
}: {
  actor: WorkContext;
  expense: ExpenseRow;
  base: string;
}) {
  const preview = await expensePostingPreview(actor, expense);
  if (preview.kind === "posted") {
    const { posted } = preview;
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
                  <Link href={recordCanonicalHref(base,{kind:"finance_account",id:String(l.id)})}>{String(l.code)} · {String(l.name)}</Link>
                </td>
                <td>{minorToDecimal(Number(l.debit_minor))}</td>
                <td>{minorToDecimal(Number(l.credit_minor))}</td>
              </tr>
            ))}
          </tbody>
        </Table></TableFrame>
        <Link href={canonicalUiHref(`${base}/finance/journals/${expense.journal_id}`)}>Open posted journal</Link>
      </Card>
    );
  }
  if (preview.kind === "unconfigured")
    return (
      <p className="notice">
        Configure the company’s expense, tax and payable accounts before posting GL.
      </p>
    );
  const { accounting, mappings, accounts, invalid, total } = preview;
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
        <Link href={canonicalUiHref(`${base}/finance/journals/${expense.journal_id}`)}>Open posted journal</Link>
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
