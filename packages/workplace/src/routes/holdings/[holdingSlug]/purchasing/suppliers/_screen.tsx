import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { SectionWorkspace } from "@erpx/workplace/components/section-workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@erpx/feature-work_projection/lib/section-workspaces";

import { commercePresentationQueries } from "@erpx/server-commerce/public/presentation-queries";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { Table, TableFrame } from "@erpx/ui/components/foundation";
import { FeaturePage, FeatureTabs, FeatureWidget, WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { ListPagination } from "@erpx/ui/components/list-pagination";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { MasterEditor } from "@erpx/feature-commerce/components/lazy-master-editor";

import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { commercePage, commercePermissions } from "@erpx/server-commerce/public/commerce";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { redirect } from "next/navigation";
import { salesCustomerLegacyDestination } from "@erpx/server-sales/public/rules/navigation";


import { CustomerNotificationSettingsForm } from "@erpx/feature-commerce/components/commerce-controls";


import { Card } from "@erpx/ui/components/foundation";

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
    const view = "parties"!;
    let partyKind: "customer" | "supplier" = "supplier";
    {
        if (search.edit) {
            const presentation = await commercePresentationQueries(actor.holdingId);
            const edited = await presentation.commerceEntrypointFinanceFacts().editedPartyKind(search.edit, actor.companyId);
            if (edited?.kind === "customer" || edited?.kind === "supplier")
                partyKind = edited.kind;
        }
        ;
    }
    const destination = salesCustomerLegacyDestination(base, "commerce", { ...search, view, kind: partyKind });
    if (destination)
        redirect(destination);
    ;
    const data = await commercePage(actor, { ...search, view, kind: partyKind }).catch((error) => pageError(error, holdingSlug));
    const requestData = search.request
        ? await (await commercePresentationQueries(actor.holdingId)).employeeRequestDetail(actor, search.request).catch((error) => pageError(error, holdingSlug))
        : null;
    const notificationSettings = null;
    
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
    
    
    const orders = data.orders.filter((o) => o.kind === view &&
        (!search.q ||
            `${o.number} ${o.party_name} ${o.state}`.toLowerCase().includes(search.q.toLowerCase())));
    const canCreateMaster = permissions.purchase;
    const area = "Procurement";
    const pageTitle = "Suppliers";
    
    
    
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
        {false}
        {(<FeatureWidget className={undefined} appearance="standard" id="commerce-records" title={pageTitle}>
            {permissions.approve && notificationSettings && (<CustomerNotificationSettingsForm base={base} settings={notificationSettings}/>)}
            {canCreateMaster &&
                (data.options && search.new === "1" ? (<FeatureWidget appearance="standard" id="operations-sales-new-entry" title="New entry">
                  <MasterEditor base={base} kind={view as "products" | "parties" | "warehouses"} partyKind={partyKind} options={data.options} initialOpen/>
                </FeatureWidget>) : (<UiActionLink appearance="standard" variant="primary" href={`?view=${view}${`&kind=${partyKind}`}&new=1`}>
                  New entry
                </UiActionLink>))}
            <TableFrame appearance="standard" label="Records" className={"business-table-wrap"}>
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
                  {data[view as "products" | "parties" | "warehouses"].map((row) => (<tr key={String(row.id)}>
                      <td><Link href={canonicalUiHref(`${base}/record-details/${"party"}/${encodeURIComponent(String(row.id))}`)} record={{ kind: "party", id: String(row.id) }}>{String(row.sku ?? row.code)}</Link></td>
                      <td>{String(row.name)}</td>
                      <td>
                        {`${row.kind} · ${row.payment_terms_days} days${row.kind === "customer" ? ` · ${row.notification_consent}${row.notification_email ? ` · ${row.notification_email}` : ""}` : ""}`}
                      </td>
                      <td>
                        <StatusBadge appearance="standard" status={String(row.status)}/>
                      </td>
                      <td>
                        <div className="ui-form-actions">
                          {(row.kind === "supplier"
                    ? permissions.purchase
                    : permissions.sales) &&
                    (data.options && search.edit === row.id ? (<MasterEditor base={base} kind={view as "products" | "parties" | "warehouses"} partyKind={partyKind} options={data.options} row={row} initialOpen/>) : (<Link href={`?view=${view}${`&kind=${partyKind}`}&edit=${row.id}${search.cursor ? `&cursor=${encodeURIComponent(search.cursor)}` : ""}`}>
                                Edit
                              </Link>))}
                          {false}
                        </div>
                      </td>
                    </tr>))}
                </tbody>
              </Table>
            </TableFrame>
            {false}
            <ListPagination {...data.page} query={search} count={data.page.items.length}/>
          </FeatureWidget>)}
      </WidgetGrid></SectionWorkspace>
    </FeaturePage>);
}

