import { commercePurchaseControls } from "@erpx/server-commerce/public/purchase-controls";
import { CommerceEnvironment } from "@erpx/feature-commerce/components/purchase-context";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import catalogStyles from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/commerce/catalog.module.css";
import { SectionWorkspace } from "@erpx/workplace/components/section-workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@erpx/feature-work_projection/lib/section-workspaces";

import { commercePresentationQueries } from "@erpx/server-commerce/public/presentation-queries";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { Button, Field, Table, TableFrame, TextInput } from "@erpx/ui/components/foundation";
import { FeaturePage, FeatureTabs, FeatureWidget, WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { MoneyAmount } from "@erpx/feature-finance/components/money/currency-provider";
import { ListPagination } from "@erpx/ui/components/list-pagination";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { OrderEditor } from "@erpx/workplace/components/work-editors";
import { commerceNavigationViews } from "@erpx/feature-work_projection/lib/workplace-navigation";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { commercePage, commercePermissions } from "@erpx/server-commerce/public/commerce";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { redirect } from "next/navigation";
import { salesCustomerLegacyDestination } from "@erpx/server-sales/public/rules/navigation";




import { Card, FilterBar } from "@erpx/ui/components/foundation";
import {CommercePostingSetupRecovery} from "@erpx/feature-commerce/components/commerce-posting-setup";
import {ApiError} from "@erpx/server-runtime/public/http";

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
    
    const permissions = commercePermissions(actor);
    const search = await searchParams;
    const view = "purchase"!;
    const partyKind: "customer" | "supplier" = search.kind === "supplier" ? "supplier" : "customer";
    ;
    const destination = salesCustomerLegacyDestination(base, "commerce", { ...search, view, kind: partyKind });
    if (destination)
        redirect(destination);
    ;
    const data = await commercePage(actor, { ...search, view, kind: partyKind }).catch((error) => {
        if(error instanceof ApiError&&error.status===409&&error.code==='COMMERCE_SETUP_REQUIRED')return null;
        return pageError(error, holdingSlug);
    });
    if(data===null)return <CommercePostingSetupRecovery base={base}/>;
    const requestData = search.request
        ? await (await commercePresentationQueries(actor.holdingId)).employeeRequestDetail(actor, search.request).catch((error) => pageError(error, holdingSlug))
        : null;
    
    
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
    const orderOptions = data.options
        ? requestData
            ? {
                ...data.options,
                warehouses: data.options.warehouses.filter((w) => w.branch_id === requestData.request.branch_id),
            }
            : data.options
        : null;
    const tabs = commerceNavigationViews;
    const orders = data.orders.filter((o) => o.kind === view &&
        (!search.q ||
            `${o.number} ${o.party_name} ${o.state}`.toLowerCase().includes(search.q.toLowerCase())));
    
    const area = "Procurement";
    const pageTitle = (tabs.find(([id]) => id === view)?.[1] ?? "Records");
    
    
    
    const purchase = await commercePurchaseControls(actor,{type:"kind",kind:"purchase"});
    return (<CommerceEnvironment purchase={purchase}><FeaturePage className="ui-page-content" data-source="live">
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
      <UiActionLink appearance="standard" variant="secondary" href={`${base}/purchasing/setup/posting`}>Posting setup</UiActionLink>
      {false}
      {(<UiActionLink appearance="standard" variant="secondary" className="self-start" href={canonicalUiHref(`${base}/procurement-rfqs`)}>
          Supplier quotations &amp; award reviews
        </UiActionLink>)}
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

        {(<>
            <FeatureWidget className={catalogStyles.embedded} appearance="standard" id="commerce-records" title={"Document register"}>
              <FilterBar className="ui-filter-bar">
                <TextInput appearance="standard" type="hidden" name="view" value={view}/>
                <Field appearance="standard">
                  Search orders
                  <TextInput appearance="standard" name="q" defaultValue={search.q} placeholder="Order, customer or status"/>
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
                    {orders.map((o) => (<tr key={String(o.id)}>
                        <td>
                          <Link href={canonicalUiHref(`${base}/commerce/orders/${o.id}?view=${view}`)}>
                            {String(o.number)}
                          </Link>
                        </td>
                        <td>{String(o.party_name)}</td>
                        <td>{String(o.due_date)}</td>
                        <td data-align="right" className="whitespace-nowrap">
                          <MoneyAmount units={(BigInt(String(o.subtotal_minor)) + BigInt(String(o.tax_minor))).toString()} currency={String(o.currency)} date={String(o.order_date)} legacyScale={2}/>
                        </td>
                        <td>
                          <StatusBadge appearance="standard" status={String(o.state)}/>
                        </td>
                      </tr>))}
                  </tbody>
                </Table>
              </TableFrame>
              {!orders.length && <p className="muted">No orders match this view.</p>}
              <ListPagination {...data.page} query={search} count={data.page.items.length}/>
            </FeatureWidget>
            {(permissions.purchase) &&
                (!requestData || !!sourceRequest) &&
                (orderOptions ? (<FeatureWidget appearance="standard" id="operations-sales-order-details" title="Order details">
                  <OrderEditor drawer base={base} options={orderOptions} kind={view} sourceRequest={sourceRequest}/>
                </FeatureWidget>) : (<UiActionLink appearance="standard" variant="primary" href={`?view=${view}&new=1`}>
                  New document
                </UiActionLink>))}
          </>)}
        {false}
        {false}
      </WidgetGrid></SectionWorkspace>
    </FeaturePage></CommerceEnvironment>);
}

