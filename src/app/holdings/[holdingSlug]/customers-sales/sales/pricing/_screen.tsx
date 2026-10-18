import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { SectionWorkspace } from "@/components/workplace/section-workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@/lib/work_projection/section-workspaces";
import type { RouteSearch } from "@/lib/shared/route-context";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { FeaturePage } from "@/components/shared/features/feature-page";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";

import { redirect } from "next/navigation";
import { salesCustomerLegacyDestination } from "@/server/modules/sales/public/rules/navigation";
import { SalesPricingConsole } from "@/components/sales/sales-pricing-console";
import { pricingWorkspace } from "@/server/modules/sales/public/sales-pricing";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";


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
    
    
    const search = await searchParams;
    const view = "pricing"!;
    const partyKind: "customer" | "supplier" = search.kind === "supplier" ? "supplier" : "customer";
    ;
    const destination = salesCustomerLegacyDestination(base, "commerce", { ...search, view, kind: partyKind });
    if (destination)
        redirect(destination);
    return <PriceListPage actor={actor} base={base} query={search}/>;
}
async function PriceListPage({ actor, base, query }: {
    actor: WorkContext;
    base: string;
    query: RouteSearch;
}) {
    const data = await pricingWorkspace(actor);
    return (<FeaturePage className="ui-page-content" data-source="live">
      <header className="ui-section-header">
        <div>
          <p className="eyebrow">Sales pricing control</p>
          <h1>Price lists and discounts</h1>
          <p className="muted">
            Approved prices, effective dates, customer scope, and independent discount authority
          </p>
        </div>
        <UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/commerce?view=sales`)}>
          Sales orders
        </UiActionLink>
      </header>
      <SectionWorkspace nested showHeading={false} title="Products, warehouses & pricing" selected="pricing" items={workspaceTopicLinks(base, query, sectionWorkspaceTopics.catalog)} context="Approved price lists and discounts"><SalesPricingConsole base={base} data={data}/></SectionWorkspace>
    </FeaturePage>);
}
