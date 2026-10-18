import { forbidden } from "next/navigation";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { queryValue, type HubQuery, type CustomersView } from "@/server/modules/sales/public/rules/navigation";
import { customerReviewCounts, salesCustomersAccess } from "@/server/modules/sales/public/sales-customers";
import { HubShell } from "@/components/sales/customers/hub-shell";
import { CustomerDirectoryView } from "@/components/sales/customers/customer-views";

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
    const content = await (CustomerDirectoryView(props)).catch(error => pageError(error, holdingSlug));
    const counts = await customerReviewCounts(actor).catch(error => pageError(error, holdingSlug));
    return <HubShell base={base} hub="customers" view={view} access={access} reviewCount={counts ? counts.identity + counts.duplicates + counts.access : null}>{content}</HubShell>;
}
