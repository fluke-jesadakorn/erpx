import { Table } from "@erpx/ui/components/foundation";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { documentPostingPreview } from "@erpx/server-finance/public/document-posting-preview";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { Card, TableFrame } from "@erpx/ui/components/foundation";
export async function DocumentPostingPreview({
  actor,
  kind,
  id,
}: {
  actor: WorkContext;
  kind: "invoice" | "stock_count";
  id: string;
}) {
  const data = await documentPostingPreview(actor, kind, id);
  if (!data) return null;
  if (data.state === "no-charge-confirmed") return <Card as="section" appearance="standard" className="ui-panel"><h2>Service No-Charge Confirmation</h2><p>The covered visit has no customer amount to collect. Its original billing evidence is retained.</p></Card>;
  if (data.state === "voided") return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>Voided draft</h2>
      <p>This draft was not posted. Review its source document for the correction history.</p>
    </Card>
  );
  if (data.state === "configuration-required") return (
    <p className="notice">Configure inventory and variance account 5400 before GL posting.</p>
  );
  const { accounts, lines, debit, credit } = data;
  return (
    <Card as="section" appearance="standard" className="ui-panel">
      <h2>{data.posted ? "Posted GL" : "GL preview"} · Debit / Credit</h2>
      {!lines.length ? (
        <p>No value adjustment; no GL journal is required.</p>
      ) : (
        <TableFrame appearance="standard" label="Records"><Table className="business-table">
          <thead>
            <tr>
              <th>Account</th>
              <th>Debit</th>
              <th>Credit</th>
            </tr>
          </thead>
          <tbody>
            {lines.map((l, i) => {
              const account = accounts.find((a) => a.id === l.id);
              return (
                <tr key={i}>
                  <td>
                    {account ? `${account.code} · ${account.name}` : "Missing account mapping"}
                  </td>
                  <td>{minorToDecimal(l.debit)}</td>
                  <td>{minorToDecimal(l.credit)}</td>
                </tr>
              );
            })}
          </tbody>
          <tfoot>
            <tr>
              <th>Total</th>
              <td>{minorToDecimal(debit)}</td>
              <td>{minorToDecimal(credit)}</td>
            </tr>
          </tfoot>
        </Table></TableFrame>
      )}
      {debit !== credit && (
        <p className="error">Debit and Credit do not balance. Posting is blocked.</p>
      )}
    </Card>
  );
}
