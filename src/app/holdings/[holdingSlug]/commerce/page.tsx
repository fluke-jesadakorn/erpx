import catalogStyles from "./catalog.module.css";
import {SectionWorkspace} from "@/components/workplace/section-workspace";
import {sectionWorkspaceTopics,workspaceTopicLinks} from "@/lib/work_projection/section-workspaces";
import type {RouteSearch} from "@/lib/shared/route-context";
import { commercePresentationQueries } from "@/server/modules/commerce/public/presentation-queries";


import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Button, Field, Table, TableFrame, TextInput } from "@/components/ui/foundation";
import {
  FeaturePage,
  FeatureTabs,
  FeatureWidget,
  WidgetGrid,
} from "@/components/shared/features/feature-page";
import { MoneyAmount } from "@/components/finance/money/currency-provider";
import { ListPagination } from "@/components/ui/list-pagination";
import { StatusBadge } from "@/components/ui/primitives";
import { MasterEditor, OrderEditor } from "@/components/workplace/work-editors";
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
import { SalesPricingConsole } from "@/components/sales/sales-pricing-console";
import { pricingWorkspace } from "@/server/modules/sales/public/sales-pricing";
import { CustomerNotificationSettingsForm } from "@/components/commerce/commerce-controls";
import { InventoryReorderConsole } from "@/components/inventory/inventory-reorder-console";
import { customerNotificationSettings } from "@/server/modules/sales/public/sales-customer-notifications";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { frontWorkspaceHref } from "@/server/modules/work_projection/public/front-app";
import { RecordDocumentPanel } from "@/components/files/record-document-panel";
import { MediaPublicationPanel } from "@/components/files/media-publication-panel";
import { AttachmentFeatureSurface } from "@/components/files/attachment-feature-context";
import { Card, FilterBar } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function CommercePage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
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
  const view = [
    "sales",
    "quote",
    "pricing",
    "purchase",
    "stock",
    "products",
    "parties",
    "warehouses",
  ].includes(search.view ?? "")
    ? search.view!
    : "sales";
  let partyKind: "customer" | "supplier" = search.kind === "supplier" ? "supplier" : "customer";
  if (view === "parties") {
    if (search.edit) {
      const presentation = await commercePresentationQueries(actor.holdingId);
      const edited = await presentation.commerceEntrypointFinanceFacts().editedPartyKind(search.edit,actor.companyId);
      if (edited?.kind === "customer" || edited?.kind === "supplier") partyKind = edited.kind;
    }
    if (search.kind !== partyKind) {
      const target = new URLSearchParams();
      for (const [key, value] of Object.entries(search)) if (value) target.set(key, value);
      target.delete("cursor");
      target.set("view", "parties");
      target.set("kind", partyKind);
      redirect(`${base}/commerce?${target}`);
    }
  }
  const destination = salesCustomerLegacyDestination(base, "commerce", { ...search, view, kind: partyKind });
  if (destination) redirect(destination);
  if (view === "pricing") return <PriceListPage actor={actor} base={base} query={search} />;
  const data = await commercePage(actor, { ...search, view, kind: partyKind }).catch((error) =>
    pageError(error, holdingSlug),
  );
  const requestData = search.request
    ? await (await commercePresentationQueries(actor.holdingId)).employeeRequestDetail(actor, search.request).catch((error) => pageError(error, holdingSlug))
    : null;
  const notificationSettings =
    view === "parties" && partyKind === "customer"
      ? await customerNotificationSettings(actor)
      : null;
  const reorderPolicies =
    view === "stock" && canReadCounts(actor) ? await listInventoryReorderPolicies(actor) : [];
  const sourceRequest =
    requestData?.request.kind === "purchase" &&
    requestData.actions.includes("link-order") &&
    !requestData.request.purchase_order_id
      ? {
          id: requestData.request.id,
          version: requestData.request.version,
          neededDate: requestData.request.needed_date,
          note: `${requestData.request.title} · ${requestData.request.description}`.slice(0, 2000),
        }
      : undefined;
  const orderOptions = data.options
    ? requestData
      ? {
          ...data.options,
          warehouses: data.options.warehouses.filter(
            (w) => w.branch_id === requestData.request.branch_id,
          ),
        }
      : data.options
    : null;
  const tabs = commerceNavigationViews;
  const orders = data.orders.filter(
    (o) =>
      o.kind === view &&
      (!search.q ||
        `${o.number} ${o.party_name} ${o.state}`.toLowerCase().includes(search.q.toLowerCase())),
  );
  const canCreateMaster =
    view === "parties"
      ? partyKind === "customer"
        ? permissions.sales
        : permissions.purchase
      : permissions.stock;
  const area =
    view === "parties"
      ? partyKind === "customer"
        ? "CRM"
        : "Procurement"
      : view === "purchase"
        ? "Procurement"
        : ["stock", "products", "warehouses"].includes(view)
          ? "Inventory"
          : "Sales";
  const pageTitle =
    view === "parties"
      ? partyKind === "customer"
        ? "Customer directory"
        : "Suppliers"
      : (tabs.find(([id]) => id === view)?.[1] ?? "Records");
  const selectedProduct =
    view === "products" && search.files
      ? data.products.find((row) => String(row.id) === search.files)
      : null;
  const lotRecords =
    view === "stock" && hasPermission(actor, "inventory.read")
      ? (await (await commercePresentationQueries(actor.holdingId)).commerceEntrypointInventoryFacts().lotRecords(actor.companyId,actor.branchId??null)).results
      : [];
  const selectedLot = search.lotFiles
    ? lotRecords.find((row) => String(row.id) === search.lotFiles)
    : null;
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <header className="ui-section-header">
        <div>
          <p className="eyebrow">{area}</p>
          <h1>{pageTitle}</h1>
          <p className="muted">Company records and next actions · {data.currency}</p>
        </div>
        {permissions.finance && (
          <UiActionLink appearance="standard" variant="secondary" href={`${base}/finance`}>
            Finance
          </UiActionLink>
        )}
      </header>
      {!["products","warehouses"].includes(view)&&<FeatureTabs appearance="standard" />}
      {["sales", "quote"].includes(view) && (
        <div className="ui-form-actions">
          <UiActionLink appearance="standard" variant="secondary" href={`${base}/crm`}>
            Leads &amp; opportunities
          </UiActionLink>
          <UiActionLink appearance="standard" variant="secondary" href={`${base}/commerce?view=pricing`}>
            Price lists &amp; discount controls
          </UiActionLink>
        </div>
      )}
      {view === "purchase" && (
        <UiActionLink appearance="standard" variant="secondary" className="self-start" href={`${base}/procurement-rfqs`}>
          Supplier quotations &amp; award reviews
        </UiActionLink>
      )}
      <SectionWorkspace nested showHeading={false} embedded={!["products","warehouses"].includes(view)} title="Products, warehouses & pricing" selected={view} items={workspaceTopicLinks(base,search,sectionWorkspaceTopics.catalog.filter(topic=>topic.id!=='pricing'||permissions.sales&&!actor.branchId&&!actor.departmentId))} context={pageTitle}>
      <WidgetGrid>
        {requestData && (
          <FeatureWidget appearance="standard" id="operations-sales-approved-request" title="Approved request">
            <Card as="section" appearance="standard" className="ui-panel">
              <Link href={`${base}/requests/${requestData.request.id}`}>
                {requestData.request.title}
              </Link>
              {requestData.items.map((item, i) => (
                <p key={i}>
                  {item.description} · {item.quantity} {item.unit ?? "each"}
                  {item.estimatedCost
                    ? ` · Estimate ${requestData.request.currency} ${item.estimatedCost}`
                    : ""}
                </p>
              ))}
              <p className="muted">
                {sourceRequest
                  ? "Match the requested items to products and review supplier prices. Saving creates the linked purchase order."
                  : "Return to the request to see its current handoff and next action."}
              </p>
            </Card>
          </FeatureWidget>
        )}

        {["sales", "quote", "purchase"].includes(view) && (
          <>
            <FeatureWidget className={view==='parties'?undefined:catalogStyles.embedded} appearance="standard" id="commerce-records" title={["sales", "quote", "purchase"].includes(view) ? "Document register" : pageTitle}>
              <FilterBar className="ui-filter-bar">
                <TextInput appearance="standard" type="hidden" name="view" value={view} />
                <Field appearance="standard">
                  Search orders
                  <TextInput appearance="standard"
                    name="q"
                    defaultValue={search.q}
                    placeholder="Order, customer or status"
                  />
                </Field>
                <Button appearance="standard" type="submit" variant="secondary">
                  Search
                </Button>
              </FilterBar>
              <TableFrame appearance="standard" label="Records" className="business-table-wrap">
                <Table className="business-table">
                  <thead>
                    <tr>
                      <th>Document</th>
                      <th>Customer / supplier</th>
                      <th>Required date</th>
                      <th data-align="right">Total</th>
                      <th>Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {orders.map((o) => (
                      <tr key={String(o.id)}>
                        <td>
                          <Link href={`${base}/commerce/orders/${o.id}?view=${view}`}>
                            {String(o.number)}
                          </Link>
                        </td>
                        <td>{String(o.party_name)}</td>
                        <td>{String(o.due_date)}</td>
                        <td data-align="right" className="whitespace-nowrap">
                          <MoneyAmount
                            units={(
                              BigInt(String(o.subtotal_minor)) + BigInt(String(o.tax_minor))
                            ).toString()}
                            currency={String(o.currency)}
                            date={String(o.order_date)}
                            legacyScale={2}
                          />
                        </td>
                        <td>
                          <StatusBadge appearance="standard" status={String(o.state)} />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </Table>
              </TableFrame>
              {!orders.length && <p className="muted">No orders match this view.</p>}
              <ListPagination {...data.page} query={search} count={data.page.items.length} />
            </FeatureWidget>
            {(view === "purchase" ? permissions.purchase : permissions.sales) &&
              (!requestData || !!sourceRequest) &&
              (orderOptions ? (
                <FeatureWidget appearance="standard" id="operations-sales-order-details" title="Order details">
                  <OrderEditor
                    base={base}
                    options={orderOptions}
                    kind={view}
                    sourceRequest={sourceRequest}
                  />
                </FeatureWidget>
              ) : (
                <UiActionLink appearance="standard" variant="primary" href={`?view=${view}&new=1`}>
                  New document
                </UiActionLink>
              ))}
          </>
        )}
        {view === "stock" && (
          <>
            {data.inTransit.length > 0 && (
              <Card as="section" appearance="standard" className="ui-panel">
                <h2>Stock travelling between warehouses</h2>
                {data.inTransit.map((t) => (
                  <p key={String(t.currency)}>
                    {String(t.line_count)} product lines · {String(t.currency)}{" "}
                    {minorToDecimal(Number(t.value_minor))} remain in company inventory, excluded
                    from available warehouse stock.
                  </p>
                ))}
                <Link href={`${base}/stock-transit`}>
                  Review dispatch, receiving and discrepancies
                </Link>
              </Card>
            )}
            {canReadCounts(actor) && (
              <div className="ui-form-actions">
                <UiActionLink appearance="standard" variant="primary" href={`${base}/stock-counts`}>
                  Stock counts & adjustments
                </UiActionLink>
                <UiActionLink appearance="standard" variant="secondary" href={`${base}/stock-transfers`}>
                  Warehouse transfers
                </UiActionLink>
              </div>
            )}
            {canReadCounts(actor) && (canPrepareCounts(actor) || canApproveCounts(actor)) && (
              <InventoryReorderConsole
                base={base}
                readerId={actor.userId}
                canPrepare={canPrepareCounts(actor)}
                canApprove={canApproveCounts(actor)}
                policies={reorderPolicies}
                warehouses={data.warehouses.map((w) => ({
                  id: String(w.id),
                  name: String(w.name),
                  status: String(w.status),
                }))}
                products={data.products.map((p) => ({
                  id: String(p.id),
                  name: String(p.name),
                  sku: String(p.sku),
                  unit: String(p.unit),
                  status: String(p.status),
                }))}
              />
            )}
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
                        const available = Math.max(
                          0,
                          Number(s.quantity_milli) -
                            Math.max(
                              Number(s.reserved_milli ?? 0),
                              Number(s.quarantined_milli ?? 0),
                            ),
                        );
                        return (
                          <tr key={`${s.warehouse_id}-${s.product_id}`}>
                            <td>
                              <Link href={`${base}/record-details/stock_position/${encodeURIComponent(String(s.product_id))}`} record={{kind:"stock_position",id:String(s.product_id),args:{warehouseId:String(s.warehouse_id)}}}>{String(s.sku)} · {String(s.name)}</Link>
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
                          </tr>
                        );
                      })}
                    </tbody>
                  </Table>
                </TableFrame>
                {!data.stock.length && (
                  <p className="muted">
                    Receive a purchase order or record and approve a physical count to establish
                    stock.
                  </p>
                )}
              </Card>
            </FeatureWidget>
            <AttachmentFeatureSurface>
              {!!lotRecords.length && (
                <FeatureWidget appearance="standard" id="operations-sales-lot-documents" title="Lot and serial documents">
                  <Card as="section" appearance="standard" className="ui-panel">
                    <p className="muted">
                      Certificates, labels, and COA files belong to the canonical lot or serial
                      record, not to the stock-balance projection.
                    </p>
                    <div className="ui-form-actions">
                      {lotRecords.map((lot) => (
                        <UiActionLink appearance="standard"
                          variant="secondary"
                          key={String(lot.id)}
                          href={`${base}/record-details/inventory_lot/${encodeURIComponent(String(lot.id))}`}
                        >
                          {String(lot.sku)} ·{" "}
                          {lot.serial_number
                            ? `Serial ${String(lot.serial_number)}`
                            : `Lot ${String(lot.lot_code)}`}
                        </UiActionLink>
                      ))}
                    </div>
                    {selectedLot && (
                      <RecordDocumentPanel appearance="standard"
                        base={base}
                        targetType="inventory_lot"
                        targetId={String(selectedLot.id)}
                        expectedTargetVersion={Number(selectedLot.version)}
                        editable={permissions.stock}
                        title={`${String(selectedLot.sku)} · ${selectedLot.serial_number ? `Serial ${String(selectedLot.serial_number)}` : `Lot ${String(selectedLot.lot_code)}`}`}
                        purposes={[
                          {
                            key: "coa",
                            label: "Certificate of analysis",
                            description: "Attach the COA issued for this exact lot or serial.",
                          },
                          {
                            key: "certificate",
                            label: "Certificate",
                            description:
                              "Attach the applicable conformity, origin, calibration, or other controlled certificate.",
                          },
                          {
                            key: "label_photo",
                            label: "Label photo",
                            description:
                              "Attach a clear label or packaging photo for traceability.",
                          },
                        ]}
                      />
                    )}
                  </Card>
                </FeatureWidget>
              )}
            </AttachmentFeatureSurface>
            <FeatureWidget appearance="standard" id="operations-sales-movement-history" title="Movement history">
              <Card as="section" appearance="standard" className="ui-panel">
                {data.moves.map((m) => (
                  <Link
                    className="ui-record-item"
                    key={String(m.id)}
                    href={
                      String(m.source_type).startsWith("pos_")
                        ? `${frontPosHref ?? `${base}/pos`}?movement=${m.id}`
                        : m.source_type === "pms"
                          ? `${base}/pms/inventory?property=${m.source_id}`
                          : m.source_type === "transit"
                            ? `${base}/stock-transit/${m.source_id}`
                            : m.source_type === "transfer"
                              ? `${base}/stock-transfers/${m.source_id}`
                              : m.source_type === "stock_count"
                                ? `${base}/stock-counts/${m.source_id}`
                                : `${base}/commerce/orders/${m.order_id}`
                    }
                  >
                    <strong>{String(m.sku)}</strong>
                    <span>
                      {Number(m.quantity_milli) / 1000} units · {String(m.warehouse_name)}
                    </span>
                    <span>
                      {String(m.order_number)} · {String(m.fulfillment_date)}
                    </span>
                  </Link>
                ))}
                {data.movementPage && (
                  <ListPagination
                    {...data.movementPage}
                    cursorName="movementCursor"
                    query={search}
                    count={data.movementPage.items.length}
                  />
                )}
              </Card>
            </FeatureWidget>
          </>
        )}
        {["products", "parties", "warehouses"].includes(view) && (
          <FeatureWidget className={view==='parties'?undefined:catalogStyles.embedded} appearance="standard" id="commerce-records" title={["sales", "quote", "purchase"].includes(view) ? "Document register" : pageTitle}>
            {view === "parties" && permissions.approve && notificationSettings && (
              <CustomerNotificationSettingsForm base={base} settings={notificationSettings} />
            )}
            {canCreateMaster &&
              (data.options && search.new === "1" ? (
                <FeatureWidget appearance="standard" id="operations-sales-new-entry" title="New entry">
                  <MasterEditor
                    base={base}
                    kind={view as "products" | "parties" | "warehouses"}
                    partyKind={view === "parties" ? partyKind : undefined}
                    options={data.options}
                    initialOpen
                  />
                </FeatureWidget>
              ) : (
                <UiActionLink appearance="standard"
                  variant="primary"
                  href={`?view=${view}${view === "parties" ? `&kind=${partyKind}` : ""}&new=1`}
                >
                  New entry
                </UiActionLink>
              ))}
            <TableFrame appearance="standard" label="Records" className={view==='parties'?"business-table-wrap":`business-table-wrap ${catalogStyles.register}`}>
              <Table className="business-table">
                <thead>
                  <tr>
                    <th>Code</th>
                    <th>Name</th>
                    <th>Details</th>
                    <th>Status</th>
                    <th>Action</th>
                  </tr>
                </thead>
                <tbody>
                  {data[view as "products" | "parties" | "warehouses"].map((row) => (
                    <tr key={String(row.id)}>
                      <td><Link href={`${base}/record-details/${view === "products" ? "product" : view === "parties" ? "party" : "warehouse"}/${encodeURIComponent(String(row.id))}`} record={{kind:view === "products" ? "product" : view === "parties" ? "party" : "warehouse",id:String(row.id)}}>{String(row.sku ?? row.code)}</Link></td>
                      <td>{String(row.name)}</td>
                      <td>
                        {view === "products"
                          ? `${data.currency} ${minorToDecimal(Number(row.price_minor))} / ${row.unit} · ${row.tracking_mode === "none" ? "untracked" : `${row.tracking_mode} tracked`}`
                          : view === "parties"
                            ? `${row.kind} · ${row.payment_terms_days} days${row.kind === "customer" ? ` · ${row.notification_consent}${row.notification_email ? ` · ${row.notification_email}` : ""}` : ""}`
                            : String(row.branch_id)}
                      </td>
                      <td>
                        <StatusBadge appearance="standard" status={String(row.status)} />
                      </td>
                      <td>
                        <div className="ui-form-actions">
                          {(view === "parties"
                            ? row.kind === "supplier"
                              ? permissions.purchase
                              : permissions.sales
                            : permissions.stock) &&
                            (data.options && search.edit === row.id ? (
                              <MasterEditor
                                base={base}
                                kind={view as "products" | "parties" | "warehouses"}
                                partyKind={view === "parties" ? partyKind : undefined}
                                options={data.options}
                                row={row}
                                initialOpen
                              />
                            ) : (
                              <Link
                                href={`?view=${view}${view === "parties" ? `&kind=${partyKind}` : ""}&edit=${row.id}${search.cursor ? `&cursor=${encodeURIComponent(search.cursor)}` : ""}`}
                              >
                                Edit
                              </Link>
                            ))}
                          {view === "products" && (
                            <AttachmentFeatureSurface>
                              <Link
                                href={`?view=products&files=${encodeURIComponent(String(row.id))}${search.cursor ? `&cursor=${encodeURIComponent(search.cursor)}` : ""}`}
                              >
                                Files
                              </Link>
                            </AttachmentFeatureSurface>
                          )}
                        </div>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </TableFrame>
            {selectedProduct && (
              <>
                <RecordDocumentPanel appearance="standard"
                  base={base}
                  targetType="product"
                  targetId={String(selectedProduct.id)}
                  expectedTargetVersion={Number(selectedProduct.version)}
                  editable={permissions.stock}
                  title={`${String(selectedProduct.sku)} · product media and documents`}
                  purposes={[
                    {
                      key: "product_image",
                      label: "Product images",
                      description:
                        "Attach private product, label, packaging, or catalog-source images. Public use requires a separately approved sanitized publication.",
                    },
                    {
                      key: "specification",
                      label: "Product specification",
                      description:
                        "Attach the controlled product specification, data sheet, or approved instructions.",
                    },
                  ]}
                />
                <MediaPublicationPanel appearance="standard"
                  base={base}
                  targetType="product"
                  targetId={String(selectedProduct.id)}
                  purposeKey="product_image"
                  title={`${String(selectedProduct.sku)} · approved storefront media`}
                  canCreate={permissions.stock}
                  canReview={permissions.approve}
                />
              </>
            )}
            <ListPagination {...data.page} query={search} count={data.page.items.length} />
          </FeatureWidget>
        )}
      </WidgetGrid></SectionWorkspace>
    </FeaturePage>
  );
}

async function PriceListPage({ actor, base, query }: { actor: WorkContext; base: string; query:RouteSearch }) {
  const data = await pricingWorkspace(actor);
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <header className="ui-section-header">
        <div>
          <p className="eyebrow">Sales pricing control</p>
          <h1>Price lists and discounts</h1>
          <p className="muted">
            Approved prices, effective dates, customer scope, and independent discount authority
          </p>
        </div>
        <UiActionLink appearance="standard" variant="secondary" href={`${base}/commerce?view=sales`}>
          Sales orders
        </UiActionLink>
      </header>
      <SectionWorkspace nested showHeading={false} title="Products, warehouses & pricing" selected="pricing" items={workspaceTopicLinks(base,query,sectionWorkspaceTopics.catalog)} context="Approved price lists and discounts"><SalesPricingConsole base={base} data={data} /></SectionWorkspace>
    </FeaturePage>
  );
}
