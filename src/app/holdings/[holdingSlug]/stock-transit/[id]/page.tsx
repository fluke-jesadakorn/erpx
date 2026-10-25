import { recordCanonicalHref } from "@/lib/record-drawer";
import { Table } from "@/components/ui/foundation";
import { RecordLink as Link } from "@/components/records/record-link";
import { WorkReturn } from "@/components/records/loaders/workplace--work-return";
import { TransitNextAction } from "@/components/workplace/transit-next-action";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { transitDetail, transitWorkItem } from "@/server/modules/inventory/public/inventory-transit";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { minorToDecimal } from "@/lib/finance-money";
import { TransitConsole } from "@/components/records/loaders/modules--transit-console";
import { PageContent, Card, TableFrame } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor)),
    data = await transitDetail(actor, id).catch((e) => pageError(e, holdingSlug)),
    d = data.transit;
  return (
    <PageContent as="div" className="ui-page-content">
      <WorkReturn base={base} />
      <Link href={`${base}/stock-transit`}>← Stock in transit</Link>
      <h1>{String(d.number)} · Transport</h1>
      <Card as="section" appearance="standard" className="ui-panel">
        <p>
          {String(d.source_name)} → {String(d.target_name)} · {String(d.status)}
        </p>
        <p>
          Expected arrival {String(d.due_date)} ·{" "}
          {d.tracking_reference
            ? `Tracking ${d.tracking_reference}`
            : "Awaiting dispatch reference"}
        </p>
        <Link href={`${base}/stock-transfers/${d.transfer_id}`}>Approved source transfer</Link>
        <TableFrame appearance="standard" label="Records"><Table className="business-table">
          <thead>
            <tr>
              <th>Product</th>
              <th>Approved</th>
              <th>Received</th>
              <th>Approved loss</th>
              <th>Disputed</th>
              <th>Remaining transport value ({String(d.currency)})</th>
            </tr>
          </thead>
          <tbody>
            {data.lines.map((l) => (
              <tr key={String(l.id)}>
                <td>
                  <Link href={recordCanonicalHref(base,{kind:"product",id:String(l.product_id)})}>{String(l.sku)} · {String(l.name)}</Link>
                </td>
                <td>{Number(l.quantity_milli) / 1000}</td>
                <td>{Number(l.received_milli) / 1000}</td>
                <td>{Number(l.lost_milli) / 1000}</td>
                <td>{Number(l.pending_loss_milli) / 1000}</td>
                <td>
                  {d.status === "awaiting_dispatch"
                    ? "Not dispatched"
                    : d.status === "cancelled"
                      ? "Cancelled"
                      : minorToDecimal(Number(l.value_minor) - Number(l.settled_value_minor))}
                </td>
              </tr>
            ))}
          </tbody>
        </Table></TableFrame>
        <p>
          Disputed quantities remain in transit until Finance approves a loss. Receiving into a
          quarantined location does not release its quality hold.
        </p>
      </Card>
      {data.lots.length > 0 && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Lot and serial custody</h2>
          <p className="muted">
            Receiving staff select the physical lot or serial identifiers for each partial receipt
            or loss report; each identifier retains its individual cost.
          </p>
          <TableFrame appearance="standard" label="Records"><Table className="business-table">
            <thead>
              <tr>
                <th>Lot / serial</th>
                <th>Dispatched</th>
                <th>Received</th>
                <th>Approved loss</th>
                <th>Disputed</th>
                <th>Remaining</th>
              </tr>
            </thead>
            <tbody>
              {data.lots.map((lot) => (
                <tr key={String(lot.id)}>
                  <td>
                    <Link href={recordCanonicalHref(base,{kind:"inventory_lot",id:String(lot.lot_id)})}>{lot.serial_number
                      ? `Serial ${String(lot.serial_number)}`
                      : `Lot ${String(lot.lot_code)}`}</Link>
                    {lot.expiry_date ? ` · expires ${String(lot.expiry_date)}` : ""}
                  </td>
                  <td>{Number(lot.quantity_milli) / 1000}</td>
                  <td>{Number(lot.received_milli) / 1000}</td>
                  <td>{Number(lot.lost_milli) / 1000}</td>
                  <td>{Number(lot.pending_loss_milli) / 1000}</td>
                  <td>
                    {(Number(lot.quantity_milli) -
                      Number(lot.received_milli) -
                      Number(lot.lost_milli) -
                      Number(lot.pending_loss_milli)) /
                      1000}
                  </td>
                </tr>
              ))}
            </tbody>
          </Table></TableFrame>
        </Card>
      )}
      <TransitNextAction item={transitWorkItem(actor, d)} base={base} />
      <TransitConsole base={base} data={data} />
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Immutable movement and decision evidence</h2>
        {data.events.map((e) => (
          <article key={String(e.id)}>
            <h3>
              {String(e.kind)} · {String(e.movement_date)}
            </h3>
            <p>{String(e.evidence)}</p>
            <p>
              {Number(e.quantity_milli) / 1000} units · {String(d.currency)}{" "}
              {minorToDecimal(Number(e.value_minor))} · {String(e.actor_id)}
            </p>
            {e.resolution && <p>Resolution: {String(e.resolution)}</p>}
            {e.journal_id && (
              <Link href={`${base}/finance/journals/${e.journal_id}`}>
                Source-linked loss journal
              </Link>
            )}
          </article>
        ))}
        {!data.events.length && <p>No physical movement recorded yet.</p>}
      </Card>
    </PageContent>
  );
}
