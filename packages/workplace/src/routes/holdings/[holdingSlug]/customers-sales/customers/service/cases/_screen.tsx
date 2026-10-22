import { forbidden } from "next/navigation";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { queryValue, type HubQuery, type CustomersView } from "@erpx/server-sales/public/rules/navigation";
import { customerReviewCounts, salesCustomersAccess } from "@erpx/server-sales/public/sales-customers";
import { HubShell } from "@erpx/feature-sales/components/customers/hub-shell";
import { CustomerSupportView } from "@erpx/feature-sales/components/customers/customer-views";

export default async function Page({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<HubQuery>;
}) {
    const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), access = await salesCustomersAccess(actor), query = await searchParams;
    const view = (queryValue(query, "view") || (["directory", "contacts", "support", "reviews"] as const).find(view => access[view])) as CustomersView;
    if (!access[view])
        forbidden();
    ;
    const props = { actor, base, query: { ...query, view } };
    const content = await (CustomerSupportView(props)).catch(error => pageError(error, holdingSlug));
    const counts = await customerReviewCounts(actor).catch(error => pageError(error, holdingSlug));
    return <HubShell base={base} hub="customers" view={view} access={access} reviewCount={counts ? counts.identity + counts.duplicates + counts.access : null}>{content}</HubShell>;
}
