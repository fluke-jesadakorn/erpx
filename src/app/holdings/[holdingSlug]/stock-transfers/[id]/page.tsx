import { recordCanonicalHref } from "@/lib/records/record-drawer";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Table, TableFrame } from "@/components/ui/foundation";
import { TimeText } from "@/components/people_payroll/time/time-preference-context";
import {
  FeaturePage,
  FeatureTabs,
  FeatureWidget,
  WidgetGrid,
} from "@/components/shared/features/feature-page";
import { TransferActions, TransferEditor } from "@/components/records/loaders/modules--stock-transfer-controls";
import { StatusBadge } from "@/components/ui/primitives";
import { FlowPanel, documentFlow } from "@/components/workplace/flow-panel";
import { RecordNextAction } from "@/components/workplace/record-next-action";
import { WorkReturn } from "@/components/records/loaders/workplace--work-return";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import {
  canPrepareTransfers,
  canEditTransfer,
  canReviewTransfer,
  transferDetail,
  transferOptions,
} from "@/server/modules/inventory/public/inventory-transfers";
import { getTransit, transitWorkItem } from "@/server/modules/inventory/public/inventory-transit";
import { TransitNextAction } from "@/components/workplace/transit-next-action";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { RecordLink as Link } from "@/components/records/record-link";
import { Card } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function Transfer({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor));
  const [{ transfer, lines, allocations, decisions, history }, options] = await Promise.all([
    transferDetail(actor, id).catch((e) => {
      return pageError(e, holdingSlug);
    }),
    canPrepareTransfers(actor)
      ? transferOptions(actor)
      : Promise.resolve({ warehouses: [], sources: [], stock: [], lots: [] }),
  ]);
  const editable = canEditTransfer(actor, transfer),
    stale =
      lines.some((l) => Number(l.current_version) !== Number(l.source_version)) ||
      allocations.some(
        (a) => Number(a.current_balance_version) !== Number(a.source_balance_version),
      );
  const flow = await documentFlow(actor, "transfer", id);
  const transport =
    transfer.delivery_mode === "transit" && transfer.status === "posted"
      ? transitWorkItem(actor, await getTransit(actor, `transit-${id}`))
      : null;
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <WorkReturn base={base} />
      <Link className="back-link" href={`${base}/stock-transfers`}>
        ← Warehouse transfers
      </Link>
      <div className="ui-section-header">
        <div>
          <p className="eyebrow">{String(transfer.number)}</p>
          <h1>Warehouse transfer</h1>
          <p>
            {String(transfer.source_warehouse_name)} → {String(transfer.target_warehouse_name)}
          </p>
          <p className="muted">
            {String(transfer.transfer_date)} · {String(transfer.reason)}
          </p>
        </div>
        <StatusBadge appearance="standard"
          status={
            transfer.delivery_mode === "transit" && transfer.status === "posted"
              ? "transport_approved"
              : String(transfer.status)
          }
        />
      </div>
      {transfer.delivery_mode === "transit" && transfer.status === "posted" && (
        <UiActionLink appearance="standard" variant="primary" href={`${base}/stock-transit/transit-${id}`}>
          Open dispatch, receipt and discrepancies
        </UiActionLink>
      )}
      <FeatureTabs appearance="standard" />
      <WidgetGrid pageKey="stock-transfers-detail">
        {transport ? (
          <TransitNextAction item={transport} base={base} />
        ) : (
          <RecordNextAction flow={flow} />
        )}
        <TransferActions
          base={base}
          transfer={transfer}
          editable={editable}
          reviewable={canReviewTransfer(actor, transfer) && flow.canAct}
          stale={stale}
        />
        <section className="card-grid">
          <Card as="article" appearance="standard" className="ui-panel">
            <span className="eyebrow">Products</span>
            <strong>{lines.length}</strong>
          </Card>
          <Card as="article" appearance="standard" className="ui-panel">
            <span className="eyebrow">
              {transfer.delivery_mode === "transit" ? "Approved stock value" : "Stock value moved"}
            </span>
            <strong>
              {String(transfer.currency)} {minorToDecimal(Number(transfer.total_value_minor))}
            </strong>
          </Card>
        </section>
        <FeatureWidget
          id="stock-transfers-detail-transfer-quantities-and-valuation"
          title="Transfer quantities and valuation"
        >
          <Card as="section" appearance="standard" className="ui-panel">
            <TableFrame appearance="standard" label="Records" className="business-table-wrap">
              <Table className="business-table">
                <thead>
                  <tr>
                    <th>Product</th>
                    <th>Traceability</th>
                    <th>Transfer quantity</th>
                    <th>Recorded source quantity</th>
                    <th>Value ({String(transfer.currency)})</th>
                  </tr>
                </thead>
                <tbody>
                  {lines.map((l) => (
                    <tr key={String(l.id)}>
                      <td>
                        <Link href={recordCanonicalHref(base,{kind:"product",id:String(l.product_id)})}>{String(l.sku)} · {String(l.description)}</Link>
                      </td>
                      <td>
                        {l.tracking_mode === "none"
                          ? "None"
                          : allocations
                              .filter((a) => a.transfer_line_id === l.id)
                              .map((a) => <span key={String(a.id)}><Link href={recordCanonicalHref(base,{kind:"inventory_lot",id:String(a.lot_id)})}>{a.serial_number ? `Serial ${a.serial_number}` : `Lot ${a.lot_code}`}</Link>{" "}</span>)}
                      </td>
                      <td>
                        {Number(l.quantity_milli) / 1000} {String(l.unit)}
                      </td>
                      <td>
                        {Number(l.source_quantity_milli) / 1000} {String(l.unit)}
                      </td>
                      <td>{minorToDecimal(Number(l.value_minor))}</td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </TableFrame>
            <p className="muted">
              {transfer.delivery_mode === "transit"
                ? "Approval does not move stock. Dispatch removes the selected lots or serials from the source into transit; each receipt or independently approved loss settles the transport balance."
                : "Company inventory value is unchanged. Each outbound lot quantity and value has an equal receiving movement."}
            </p>
            <Link href={`${base}/commerce?view=stock`}>Open stock and movement history</Link>
          </Card>
        </FeatureWidget>
        {editable && (
          <FeatureWidget appearance="standard" id="stock-transfers-detail-warehouse-transfer" title="Warehouse transfer">
            <TransferEditor
              key={Number(transfer.version)}
              base={base}
              options={options}
              transfer={transfer}
              lines={lines}
              allocations={allocations}
            />
          </FeatureWidget>
        )}
        <FeatureWidget
          id="stock-transfers-detail-receipt-and-review-decisions"
          title="Receipt and review decisions"
        >
          <Card as="section" appearance="standard" className="ui-panel">
            {!decisions.length ? (
              <p className="muted">No decisions yet.</p>
            ) : (
              decisions.map((d) => (
                <div className="ui-record-item" key={String(d.id)}>
                  <div className="row-main">
                    <strong>
                      {String(d.action)} · {String(d.actor_name)}
                    </strong>
                    <p>{String(d.reason)}</p>
                    <span className="muted">
                      Reviewed version {Number(d.reviewed_version)} ·{" "}
                      <TimeText appearance="standard" value={Number(d.created_at)} />
                    </span>
                  </div>
                </div>
              ))
            )}
          </Card>
        </FeatureWidget>
        <FeatureWidget appearance="standard" id="stock-transfers-detail-activity" title="Activity">
          <Card as="section" appearance="standard" className="ui-panel">
            {history.map((h) => (
              <div className="ui-record-item" key={String(h.id)}>
                <div className="row-main">
                  <strong>{String(h.action).replaceAll(".", " ")}</strong>
                  <div className="muted">
                    {String(h.actor_name)} · <TimeText appearance="standard" value={Number(h.created_at)} />
                  </div>
                </div>
              </div>
            ))}
          </Card>
        </FeatureWidget>
        <FlowPanel flow={flow} actor={actor} kind="transfer" id={id} base={base} />
      </WidgetGrid>
    </FeaturePage>
  );
}
