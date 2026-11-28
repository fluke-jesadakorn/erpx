import { forbidden } from "next/navigation";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { queryValue, type HubQuery, type SalesView } from "@erpx/sales-server/public/rules/navigation";
import { salesCustomersAccess } from "@erpx/sales-server/public/sales-customers";
import { HubShell } from "@erpx/sales-ui/components/customers/hub-shell";
import { SalesReportsView } from "@erpx/sales-ui/components/customers/sales-views";

export default async function Page({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<HubQuery>;
}) {
    const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), access = await salesCustomersAccess(actor), query = await searchParams;
    const requested = queryValue(query, "view"), defaultView = (["today", "pipeline", "documents"] as const).find(view => access[view]);
    const view = (requested || defaultView) as SalesView;
    if (!access[view])
        forbidden();
    ;
    const props = { actor, base, query: { ...query, view } };
    const content = await (SalesReportsView(props)).catch(error => pageError(error, holdingSlug));
    return <HubShell base={base} hub="sales" view={view} access={access}>{content}</HubShell>;
}
