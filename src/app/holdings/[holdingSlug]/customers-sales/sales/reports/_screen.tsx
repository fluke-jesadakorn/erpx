import { forbidden } from "next/navigation";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { queryValue, type HubQuery, type SalesView } from "@/server/modules/sales/public/rules/navigation";
import { salesCustomersAccess } from "@/server/modules/sales/public/sales-customers";
import { HubShell } from "@/components/sales/customers/hub-shell";
import { SalesReportsView } from "@/components/sales/customers/sales-views";

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
