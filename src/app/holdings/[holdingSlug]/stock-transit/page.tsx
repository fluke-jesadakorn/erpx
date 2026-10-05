import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Table } from "@/components/ui/foundation";
import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { listTransit } from "@/server/inventory-transit";
import { pageError } from "@/server/page-error";
import { PageContent, Card, TableFrame } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function Page({ params }: { params: Promise<{ holdingSlug: string }> }) {
  const { holdingSlug } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor)),
    rows = await listTransit(actor).catch((e) => pageError(e, holdingSlug));
  return (
    <PageContent as="div" className="ui-page-content">
      <h1>Stock in transit</h1>
      <p>
        Approved transport, source dispatch, partial receiving and independently reviewed
        discrepancies. Goods remain in company inventory while in transit.
      </p>
      <UiActionLink appearance="standard" variant="primary" href={`${base}/stock-transfers`}>
        Prepare or approve warehouse transfers
      </UiActionLink>
      <Card as="section" appearance="standard" className="ui-panel">
        <TableFrame appearance="standard" label="Records"><Table className="business-table">
          <thead>
            <tr>
              <th>Transfer</th>
              <th>Route</th>
              <th>Expected arrival</th>
              <th>Status</th>
              <th>Pending discrepancies</th>
            </tr>
          </thead>
          <tbody>
            {rows.map((d) => (
              <tr key={String(d.id)}>
                <td>
                  <Link href={`${base}/stock-transit/${d.id}`}>{String(d.number)}</Link>
                </td>
                <td>
                  {String(d.source_name)} → {String(d.target_name)}
                </td>
                <td>{String(d.due_date)}</td>
                <td>{String(d.status)}</td>
                <td>{Number(d.pending_losses)}</td>
              </tr>
            ))}
          </tbody>
        </Table></TableFrame>
        {!rows.length && <p>No staged transport records in this scope.</p>}
      </Card>
    </PageContent>
  );
}
