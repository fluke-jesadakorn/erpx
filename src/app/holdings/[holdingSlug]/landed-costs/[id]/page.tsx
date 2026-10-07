import { recordCanonicalHref } from "@/lib/record-drawer";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Table, TableFrame } from "@/components/ui/foundation";
import { TimeText } from "@/components/time/time-preference-context";
import { RecordLink as Link } from "@/components/records/record-link";
import { StatusBadge } from "@/components/ui/primitives";
import { LandedCostActions } from "@/components/modules/landed-cost-controls";
import { minorToDecimal } from "@/lib/finance-money";
import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { landedCostDetail } from "@/server/modules/inventory/public/inventory-landed-costs";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { PageContent, Card } from "@/components/ui/foundation";

export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor)),
    data = await landedCostDetail(actor, id).catch((e) => pageError(e, holdingSlug)),
    { cost, lines, lots, events } = data;
  const canApprove =
      !actor.branchId &&
      hasPermission(actor, "finance.approve") &&
      !samePerson(actor, cost.created_by),
    canCancel = hasPermission(actor, "finance.write") && samePerson(actor, cost.created_by);
  return (
    <PageContent as="div" className="ui-page-content">
      <Link className="back-link" href={`${base}/commerce/orders/${cost.order_id}`}>
        ← Purchase {String(cost.order_number)}
      </Link>
      <Link href={`${base}/work`}>Back to Work</Link>
      <header className="ui-section-header">
        <div>
          <p className="eyebrow">Inventory landed cost</p>
          <h1>{String(cost.number)}</h1>
          <p>{String(cost.description)}</p>
          <p className="muted">
            Purchase {String(cost.order_number)} · Receipt {String(cost.fulfillment_number)} ·{" "}
            {String(cost.warehouse_name)} · {String(cost.cost_date)}
          </p>
        </div>
        <StatusBadge appearance="standard" status={String(cost.status)} />
      </header>
      <Card as="section" appearance="standard" className="ui-panel">
        <p>
          <strong>
            {String(cost.currency)} {minorToDecimal(Number(cost.amount_minor))}
          </strong>{" "}
          allocated by {String(cost.allocation_method).replaceAll("_", " ")}
        </p>
        <p className="muted">
          Prepared records keep their stock and lot snapshots. Approval and posting stop if quantity
          or value changed after preparation.
        </p>
        {cost.journal_id && (
          <UiActionLink appearance="standard" variant="secondary" href={`${base}/finance/journals/${cost.journal_id}`}>
            Open posted journal
          </UiActionLink>
        )}
      </Card>
      <LandedCostActions
        key={String(cost.version)}
        base={base}
        record={cost}
        canApprove={canApprove}
        canCancel={canCancel}
      />
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Allocation by item</h2>
        <TableFrame appearance="standard" label="Records" className="business-table-wrap">
          <Table className="business-table">
            <thead>
              <tr>
                <th>Item</th>
                <th>Received</th>
                <th>Receipt value</th>
                <th>Allocated cost</th>
              </tr>
            </thead>
            <tbody>
              {lines.map((l) => (
                <tr key={String(l.id)}>
                  <td>
                    <Link href={recordCanonicalHref(base,{kind:"product",id:String(l.product_id)})}>{String(l.sku)} · {String(l.name)}</Link>
                  </td>
                  <td>
                    {Number(l.quantity_milli) / 1000} {String(l.unit)}
                  </td>
                  <td>{minorToDecimal(Number(l.receipt_value_minor))}</td>
                  <td>{minorToDecimal(Number(l.allocated_minor))}</td>
                </tr>
              ))}
            </tbody>
          </Table>
        </TableFrame>
      </Card>
      {!!lots.length && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Lot and serial allocation</h2>
          <TableFrame appearance="standard" label="Records" className="business-table-wrap">
            <Table className="business-table">
              <thead>
                <tr>
                  <th>Lot / serial</th>
                  <th>Quantity</th>
                  <th>Receipt value</th>
                  <th>Allocated cost</th>
                </tr>
              </thead>
              <tbody>
                {lots.map((l) => (
                  <tr key={String(l.id)}>
                    <td><Link href={recordCanonicalHref(base,{kind:"inventory_lot",id:String(l.lot_id)})}>{l.serial_number ? `Serial ${l.serial_number}` : `Lot ${l.lot_code}`}</Link></td>
                    <td>{Number(l.quantity_milli) / 1000}</td>
                    <td>{minorToDecimal(Number(l.receipt_value_minor))}</td>
                    <td>{minorToDecimal(Number(l.allocated_minor))}</td>
                  </tr>
                ))}
              </tbody>
            </Table>
          </TableFrame>
        </Card>
      )}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Audit history</h2>
        {events.map((e) => (
          <div className="ui-record-item" key={String(e.id)}>
            <strong>{String(e.action).replaceAll("_", " ")}</strong>
            <span>
              {String(e.reason)} · Version {String(e.reviewed_version)}
            </span>
            <time>{<TimeText appearance="standard" value={Number(e.created_at)} />}</time>
          </div>
        ))}
      </Card>
    </PageContent>
  );
}
