import {headers} from 'next/headers';
import {serviceCountWriterOwner} from '@erpx/server-inventory/public/service-initial-physical-stock-count';
import {serviceCountRead} from '../../../../../../lib/service-count-native-read';
import {countPurchaseControls} from "@erpx/server-inventory/public/count-purchases";
import {CountEnvironment} from "@erpx/feature-inventory/components/count-purchase-context";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { Table, TableFrame } from "@erpx/ui/components/foundation";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
import {
  FeaturePage,
  FeatureTabs,
  FeatureWidget,
  WidgetGrid,
} from "@erpx/feature-shared/components/features/feature-page";
import { DocumentPostingPreview } from "@erpx/feature-finance/components/document-posting-preview";
import { CountActions, CountEditor } from "@erpx/feature-records/components/loaders/modules--stock-count-controls";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { FlowPanel, documentFlow } from "@erpx/workplace/components/flow-panel";
import { RecordNextAction } from "@erpx/workplace/components/record-next-action";
import { WorkReturn } from "@erpx/feature-records/components/loaders/workplace--work-return";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import {
  canEditCount,
  canPrepareCounts,
  canReviewCount,
  countDetail,
  countOptions,
} from "@erpx/server-inventory/public/inventory-counts";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { Card } from "@erpx/ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function Count({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor));
  const serviceOwned=await serviceCountWriterOwner(actor,id).catch((e)=>pageError(e,holdingSlug));
  const requestHeaders=serviceOwned==='service'?await headers():null;
  const purchase=await countPurchaseControls(actor,id);
  const [detail, options] = await Promise.all([
    (serviceOwned==='service'?serviceCountRead(requestHeaders!,actor,holdingSlug,{kind:'generic-detail',id}):countDetail(actor,id)).catch((e) => pageError(e, holdingSlug)),
    canPrepareCounts(actor) && purchase?.newWork!==false
      ? countOptions(actor).catch((e) => pageError(e, holdingSlug))
      : Promise.resolve({ warehouses: [], products: [], stock: [], lots: [], currency: "" }),
  ]);
  const { count, lines, lotLines, history, decisions } = detail;
  const editable = canEditCount(actor, count),
    stale =
      lines.some((l) => Number(l.current_version) !== Number(l.snapshot_version)) ||
      lotLines.some((l) => Number(l.current_version) !== Number(l.snapshot_version));
  const flow = serviceOwned==='service'?await serviceCountRead(requestHeaders!,actor,holdingSlug,{kind:'flow',id}).catch((e)=>pageError(e,holdingSlug)):await documentFlow(actor, "stock_count", id);
  return (
    <CountEnvironment purchase={purchase}><FeaturePage className="ui-page-content" data-source="live">
      <WorkReturn base={base} />
      <Link className="back-link" href={canonicalUiHref(`${base}/stock-counts`)}>
        ← Physical stock counts
      </Link>
      <div className="ui-section-header">
        <div>
          <p className="eyebrow">{String(count.number)}</p>
          <h1>{String(count.warehouse_name)}</h1>
          <p className="muted">
            {String(count.count_date)} · {String(count.reason)}
          </p>
        </div>
        <StatusBadge appearance="standard" status={String(count.status)} />
      </div>
      <FeatureTabs appearance="standard" />
      <WidgetGrid pageKey="stock-counts-detail">
        <section data-critical-notice>
          <RecordNextAction flow={flow} />
        </section>
        <CountActions
          base={base}
          count={count}
          editable={editable}
          reviewable={canReviewCount(actor, count) && flow.canAct}
          stale={stale}
        />
        <section className="card-grid">
          <Card as="article" appearance="standard" className="ui-panel">
            <span className="eyebrow">Products counted</span>
            <strong>{lines.length}</strong>
          </Card>
          <Card as="article" appearance="standard" className="ui-panel">
            <span className="eyebrow">Inventory value adjustment</span>
            <strong>
              {String(count.currency)} {minorToDecimal(Number(count.net_value_minor))}
            </strong>
          </Card>
        </section>
        <FeatureWidget
          id="stock-counts-detail-counted-quantities-and-valuation"
          title="Counted quantities and valuation"
        >
          <Card as="section" appearance="standard" className="ui-panel">
            <TableFrame appearance="standard" label="Records" className="business-table-wrap">
              <Table className="business-table">
                <thead>
                  <tr>
                    <th>Product</th>
                    <th>Recorded quantity</th>
                    <th>Counted quantity</th>
                    <th>Difference</th>
                    <th>Value difference ({String(count.currency)})</th>
                  </tr>
                </thead>
                <tbody>
                  {lines.map((l) => (
                    <tr key={String(l.id)}>
                      <td>
                        <Link href={canonicalUiHref(`${base}/record-details/product/${encodeURIComponent(String(l.product_id))}`)}>{String(l.sku)} · {String(l.description)}</Link>
                      </td>
                      <td>
                        {Number(l.snapshot_quantity_milli) / 1000} {String(l.unit)}
                      </td>
                      <td>
                        {Number(l.counted_quantity_milli) / 1000} {String(l.unit)}
                      </td>
                      <td>
                        {(Number(l.counted_quantity_milli) - Number(l.snapshot_quantity_milli)) /
                          1000}
                      </td>
                      <td>
                        {minorToDecimal(
                          Number(l.counted_value_minor) - Number(l.snapshot_value_minor),
                        )}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </TableFrame>
            {count.review_reason && (
              <p>
                <strong>Review note:</strong> {String(count.review_reason)}
              </p>
            )}
            {count.journal_id &&
              [
                "holding_owner",
                "finance_preparer",
                "finance_approver",
                "treasury",
                "auditor",
              ].includes(actor.role) &&
              !actor.branchId && (
                <Link href={canonicalUiHref(`${base}/finance/journals/${count.journal_id}`)}>
                  Open inventory adjustment journal
                </Link>
              )}
          </Card>
        </FeatureWidget>
        {lotLines.length > 0 && (
          <FeatureWidget
            id="stock-counts-detail-lot-traceability"
            title="Lot and serial count evidence"
          >
            <Card as="section" appearance="standard" className="ui-panel">
              <TableFrame appearance="standard" label="Records" className="business-table-wrap">
                <Table className="business-table">
                  <thead>
                    <tr>
                      <th>Identity</th>
                      <th>Recorded</th>
                      <th>Counted</th>
                      <th>Value difference ({String(count.currency)})</th>
                      <th>Result</th>
                    </tr>
                  </thead>
                  <tbody>
                    {lotLines.map((l) => (
                      <tr key={String(l.id)}>
                        <td><Link href={canonicalUiHref(`${base}/record-details/inventory_lot/${encodeURIComponent(String(l.lot_id))}`)}>{String(l.lot_code ?? l.serial_number)}</Link></td>
                        <td>{Number(l.snapshot_quantity_milli) / 1000}</td>
                        <td>{Number(l.counted_quantity_milli) / 1000}</td>
                        <td>
                          {minorToDecimal(
                            Number(l.counted_value_minor) - Number(l.snapshot_value_minor),
                          )}
                        </td>
                        <td>
                          {l.source_lot_id
                            ? Number(l.counted_quantity_milli) === 0
                              ? "Depleted"
                              : "Adjusted"
                            : "Found during count"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </Table>
              </TableFrame>
            </Card>
          </FeatureWidget>
        )}
        {editable && (
          <FeatureWidget appearance="standard" id="stock-counts-detail-stock-count" title="Stock count">
            <CountEditor
              key={`${actor.companyId}:${actor.assignmentId}:${count.id}`}
              base={base}
              options={options}
              count={count}
              lines={lines}
              lotLines={lotLines}
            />
          </FeatureWidget>
        )}

        <FeatureWidget appearance="standard" id="stock-counts-detail-review-decisions" title="Review decisions">
          <Card as="section" appearance="standard" className="ui-panel">
            {decisions.length === 0 ? (
              <p className="muted">No review decisions yet.</p>
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
        <FeatureWidget appearance="standard" id="stock-counts-detail-activity" title="Activity">
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
        <DocumentPostingPreview actor={actor} kind="stock_count" id={id} />
        <FlowPanel flow={flow} actor={actor} kind="stock_count" id={id} base={base} />
      </WidgetGrid>
    </FeaturePage></CountEnvironment>
  );
}
