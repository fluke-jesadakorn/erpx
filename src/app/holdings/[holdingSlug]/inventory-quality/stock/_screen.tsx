import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { SectionWorkspace } from "@/components/workplace/section-workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@/lib/work_projection/section-workspaces";

import { commercePresentationQueries } from "@/server/modules/commerce/public/presentation-queries";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Table, TableFrame } from "@/components/ui/foundation";
import { FeaturePage, FeatureTabs, FeatureWidget, WidgetGrid } from "@/components/shared/features/feature-page";
import { ListPagination } from "@/components/ui/list-pagination";
import { minorToDecimal } from "@/lib/finance/finance-money";
import { hasPermission } from "@/lib/identity_access/permissions";
import { commerceNavigationViews } from "@/lib/work_projection/workplace-navigation";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { commercePage, commercePermissions } from "@/server/modules/commerce/public/commerce";
import { canApproveCounts, canPrepareCounts, canReadCounts } from "@/server/modules/inventory/public/inventory-counts";
import { listInventoryReorderPolicies } from "@/server/modules/inventory/public/inventory-reorder-policies";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { RecordLink as Link } from "@/components/records/record-link";
import { redirect } from "next/navigation";
import { salesCustomerLegacyDestination } from "@/server/modules/sales/public/rules/navigation";


import { InventoryReorderConsole } from "@/components/inventory/inventory-reorder-console";

import { frontWorkspaceHref } from "@/server/modules/work_projection/public/front-app";
import { RecordDocumentPanel } from "@/components/files/record-document-panel";
import { AttachmentFeatureSurface } from "@/components/files/attachment-feature-context";
import { Card } from "@/components/ui/foundation";

export default async function CommercePage({ params, searchParams, }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<{
        view?: string;
        kind?: string;
        q?: string;
        request?: string;
        cursor?: string;
        movementCursor?: string;
        new?: string;
        edit?: string;
        files?: string;
        lotFiles?: string;
    }>;
}) {
    const { holdingSlug } = await params;
    const actor = await requireWorkContext(holdingSlug);
    const base = workspaceBase(publicScope(actor));
    const frontPosHref = await frontWorkspaceHref(actor, "pos");
    const permissions = commercePermissions(actor);
    const search = await searchParams;
    const view = "stock"!;
    const partyKind: "customer" | "supplier" = search.kind === "supplier" ? "supplier" : "customer";
    ;
    const destination = salesCustomerLegacyDestination(base, "commerce", { ...search, view, kind: partyKind });
    if (destination)
        redirect(destination);
    ;
    const data = await commercePage(actor, { ...search, view, kind: partyKind }).catch((error) => pageError(error, holdingSlug));
    const requestData = search.request
        ? await (await commercePresentationQueries(actor.holdingId)).employeeRequestDetail(actor, search.request).catch((error) => pageError(error, holdingSlug))
        : null;
    
    const reorderPolicies = canReadCounts(actor) ? await listInventoryReorderPolicies(actor) : [];
    const sourceRequest = requestData?.request.kind === "purchase" &&
        requestData.actions.includes("link-order") &&
        !requestData.request.purchase_order_id
        ? {
            id: requestData.request.id,
            version: requestData.request.version,
            neededDate: requestData.request.needed_date,
            note: `${requestData.request.title} · ${requestData.request.description}`.slice(0, 2000),
        }
        : undefined;
    
    const tabs = commerceNavigationViews;
    const orders = data.orders.filter((o) => o.kind === view &&
        (!search.q ||
            `${o.number} ${o.party_name} ${o.state}`.toLowerCase().includes(search.q.toLowerCase())));
    
    const area = "Inventory";
    const pageTitle = (tabs.find(([id]) => id === view)?.[1] ?? "Records");
    
    const lotRecords = hasPermission(actor, "inventory.read")
        ? (await (await commercePresentationQueries(actor.holdingId)).commerceEntrypointInventoryFacts().lotRecords(actor.companyId, actor.branchId ?? null)).results
        : [];
    const selectedLot = search.lotFiles
        ? lotRecords.find((row) => String(row.id) === search.lotFiles)
        : null;
    return (<FeaturePage className="ui-page-content" data-source="live">
      <header className="ui-section-header">
        <div>
          <p className="eyebrow">{area}</p>
          <h1>{pageTitle}</h1>
          <p className="muted">Company records and next actions · {data.currency}</p>
        </div>
        {permissions.finance && (<UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/finance`)}>
            Finance
          </UiActionLink>)}
      </header>
      {<FeatureTabs appearance="standard"/>}
      {false}
      {false}
      <SectionWorkspace nested showHeading={false} embedded={!["products", "warehouses"].includes(view)} title="Products, warehouses & pricing" selected={view} items={workspaceTopicLinks(base, search, sectionWorkspaceTopics.catalog.filter(topic => topic.id !== 'pricing' || permissions.sales && !actor.branchId && !actor.departmentId))} context={pageTitle}>
      <WidgetGrid>
        {requestData && (<FeatureWidget appearance="standard" id="operations-sales-approved-request" title="Approved request">
            <Card as="section" appearance="standard" className="ui-panel">
              <Link href={canonicalUiHref(`${base}/requests/${requestData.request.id}`)}>
                {requestData.request.title}
              </Link>
              {requestData.items.map((item, i) => (<p key={i}>
                  {item.description} · {item.quantity} {item.unit ?? "each"}
                  {item.estimatedCost
                    ? ` · Estimate ${requestData.request.currency} ${item.estimatedCost}`
                    : ""}
                </p>))}
              <p className="muted">
                {sourceRequest
                ? "Match the requested items to products and review supplier prices. Saving creates the linked purchase order."
                : "Return to the request to see its current handoff and next action."}
              </p>
            </Card>
          </FeatureWidget>)}

        {false}
        {(<>
            {data.inTransit.length > 0 && (<Card as="section" appearance="standard" className="ui-panel">
                <h2>Stock travelling between warehouses</h2>
                {data.inTransit.map((t) => (<p key={String(t.currency)}>
                    {String(t.line_count)} product lines · {String(t.currency)}{" "}
                    {minorToDecimal(Number(t.value_minor))} remain in company inventory, excluded
                    from available warehouse stock.
                  </p>))}
                <Link href={canonicalUiHref(`${base}/stock-transit`)}>
                  Review dispatch, receiving and discrepancies
                </Link>
              </Card>)}
            {canReadCounts(actor) && (<div className="ui-form-actions">
                <UiActionLink appearance="standard" variant="primary" href={canonicalUiHref(`${base}/stock-counts`)}>
                  Stock counts & adjustments
                </UiActionLink>
                <UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/stock-transfers`)}>
                  Warehouse transfers
                </UiActionLink>
              </div>)}
            {canReadCounts(actor) && (canPrepareCounts(actor) || canApproveCounts(actor)) && (<InventoryReorderConsole base={base} readerId={actor.userId} canPrepare={canPrepareCounts(actor)} canApprove={canApproveCounts(actor)} policies={reorderPolicies} warehouses={data.warehouses.map((w) => ({
                    id: String(w.id),
                    name: String(w.name),
                    status: String(w.status),
                }))} products={data.products.map((p) => ({
                    id: String(p.id),
                    name: String(p.name),
                    sku: String(p.sku),
                    unit: String(p.unit),
                    status: String(p.status),
                }))}/>)}
            <FeatureWidget appearance="standard" id="operations-sales-stock-on-hand" title="Stock position">
              <Card as="section" appearance="standard" className="ui-panel">
                <p className="muted">
                  Available stock excludes the larger of reservations or quarantine because an
                  existing reservation can be quarantined. In-transit stock is shown separately
                  until receipt.
                </p>
                <TableFrame appearance="standard" label="Records" className="business-table-wrap">
                  <Table className="business-table">
                    <thead>
                      <tr>
                        <th>SKU / product</th>
                        <th>Warehouse</th>
                        <th>Traceability</th>
                        <th>On hand</th>
                        <th>Reserved</th>
                        <th>Quarantined</th>
                        <th>In transit</th>
                        <th>Available</th>
                        <th>Inventory value ({data.currency})</th>
                      </tr>
                    </thead>
                    <tbody>
                      {data.stock.map((s) => {
                const available = Math.max(0, Number(s.quantity_milli) -
                    Math.max(Number(s.reserved_milli ?? 0), Number(s.quarantined_milli ?? 0)));
                return (<tr key={`${s.warehouse_id}-${s.product_id}`}>
                            <td>
                              <Link href={canonicalUiHref(`${base}/record-details/stock_position/${encodeURIComponent(String(s.product_id))}`)} record={{ kind: "stock_position", id: String(s.product_id), args: { warehouseId: String(s.warehouse_id) } }}>{String(s.sku)} · {String(s.name)}</Link>
                            </td>
                            <td>{String(s.warehouse_name)}</td>
                            <td>{s.tracking_mode === "none" ? "None" : String(s.tracking_mode)}</td>
                            <td>
                              {Number(s.quantity_milli) / 1000} {String(s.unit)}
                            </td>
                            <td>{Number(s.reserved_milli ?? 0) / 1000}</td>
                            <td>{Number(s.quarantined_milli ?? 0) / 1000}</td>
                            <td>{Number(s.in_transit_milli ?? 0) / 1000}</td>
                            <td>{available / 1000}</td>
                            <td>{minorToDecimal(Number(s.value_minor))}</td>
                          </tr>);
            })}
                    </tbody>
                  </Table>
                </TableFrame>
                {!data.stock.length && (<p className="muted">
                    Receive a purchase order or record and approve a physical count to establish
                    stock.
                  </p>)}
              </Card>
            </FeatureWidget>
            <AttachmentFeatureSurface>
              {!!lotRecords.length && (<FeatureWidget appearance="standard" id="operations-sales-lot-documents" title="Lot and serial documents">
                  <Card as="section" appearance="standard" className="ui-panel">
                    <p className="muted">
                      Certificates, labels, and COA files belong to the canonical lot or serial
                      record, not to the stock-balance projection.
                    </p>
                    <div className="ui-form-actions">
                      {lotRecords.map((lot) => (<UiActionLink appearance="standard" variant="secondary" key={String(lot.id)} href={canonicalUiHref(`${base}/record-details/inventory_lot/${encodeURIComponent(String(lot.id))}`)}>
                          {String(lot.sku)} ·{" "}
                          {lot.serial_number
                        ? `Serial ${String(lot.serial_number)}`
                        : `Lot ${String(lot.lot_code)}`}
                        </UiActionLink>))}
                    </div>
                    {selectedLot && (<RecordDocumentPanel appearance="standard" base={base} targetType="inventory_lot" targetId={String(selectedLot.id)} expectedTargetVersion={Number(selectedLot.version)} editable={permissions.stock} title={`${String(selectedLot.sku)} · ${selectedLot.serial_number ? `Serial ${String(selectedLot.serial_number)}` : `Lot ${String(selectedLot.lot_code)}`}`} purposes={[
                        {
                            key: "coa",
                            label: "Certificate of analysis",
                            description: "Attach the COA issued for this exact lot or serial.",
                        },
                        {
                            key: "certificate",
                            label: "Certificate",
                            description: "Attach the applicable conformity, origin, calibration, or other controlled certificate.",
                        },
                        {
                            key: "label_photo",
                            label: "Label photo",
                            description: "Attach a clear label or packaging photo for traceability.",
                        },
                    ]}/>)}
                  </Card>
                </FeatureWidget>)}
            </AttachmentFeatureSurface>
            <FeatureWidget appearance="standard" id="operations-sales-movement-history" title="Movement history">
              <Card as="section" appearance="standard" className="ui-panel">
                {data.moves.map((m) => (<Link className="ui-record-item" key={String(m.id)} href={String(m.source_type).startsWith("pos_")
                    ? `${frontPosHref ?? canonicalUiHref(`${base}/pos`)}?movement=${m.id}`
                    : m.source_type === "pms"
                        ? canonicalUiHref(`${base}/pms/inventory?property=${m.source_id}`)
                        : m.source_type === "transit"
                            ? canonicalUiHref(`${base}/stock-transit/${m.source_id}`)
                            : m.source_type === "transfer"
                                ? canonicalUiHref(`${base}/stock-transfers/${m.source_id}`)
                                : m.source_type === "stock_count"
                                    ? canonicalUiHref(`${base}/stock-counts/${m.source_id}`)
                                    : canonicalUiHref(`${base}/commerce/orders/${m.order_id}`)}>
                    <strong>{String(m.sku)}</strong>
                    <span>
                      {Number(m.quantity_milli) / 1000} units · {String(m.warehouse_name)}
                    </span>
                    <span>
                      {String(m.order_number)} · {String(m.fulfillment_date)}
                    </span>
                  </Link>))}
                {data.movementPage && (<ListPagination {...data.movementPage} cursorName="movementCursor" query={search} count={data.movementPage.items.length}/>)}
              </Card>
            </FeatureWidget>
          </>)}
        {false}
      </WidgetGrid></SectionWorkspace>
    </FeaturePage>);
}

