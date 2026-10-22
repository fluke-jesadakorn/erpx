import { peopleSettingsHref } from "@erpx/feature-people_payroll/lib/people-route-context";
import { PeopleAccessWorkspace } from "@erpx/feature-identity_access/components/people-access-workspace";
import { iamPresentationQueries } from "@erpx/server-identity_access/public/presentation-context-queries";
import { BetaPeople } from "@erpx/feature-collaboration/components/people-page";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { PeopleNavigation } from "@erpx/feature-people_payroll/components/people-navigation";
import { PageHeader } from "@erpx/ui/components/primitives";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase, workspaceApiHref } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { forbidden, redirect } from "next/navigation";

export default async function PeoplePage({ params, searchParams, }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<{
        panel?: string;
        editor?: string;
        unit?: string;
        company?: string;
        display?: string;
        new?: string;
        person?: string;
        retirement?: string;
        cursor?: string;
        "archived.cursor"?: string;
        "archived.q"?: string;
        "archived.retirement"?: string;
        q?: string;
        status?: string;
        level?: string;
        department?: string;
        team?: string;
        view?: string;
        section?: string;
        member?: string;
        position?: string;
        case?: string;
        tab?: string;
        edit?: string;
    }>;
}) {
    const { holdingSlug } = await params;
    const actor = await requireWorkContext(holdingSlug);
    const search = await searchParams;
    if (search.panel === "settings")
        redirect(peopleSettingsHref(workspaceBase(publicScope(actor)), search));
    ;
    if (actor.beta)
        return <BetaPeople actor={actor} member={search.member}/>;
    const base = workspaceBase(publicScope(actor));
    ;
    {
        if (!hasPermission(actor, "iam.read"))
            forbidden();
        const { queries, actor: iam } = await iamPresentationQueries(holdingSlug);
        const data = await queries.peopleAccessSnapshot(iam).catch(error => pageError(error, holdingSlug));
        return <FeaturePage className="content-stack" data-source="live"><PageHeader appearance="standard" eyebrow="People" title="Users & Access" description="Manage internal accounts, external organizations and teams, and scoped access."/><PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="users"/><PeopleAccessWorkspace initial={data} initialView={"teams" as "users" | "organizations" | "teams" | "roles"} api={workspaceApiHref(publicScope(actor), "iam/people-access")}/></FeaturePage>;
    }
}
