import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@erpx/work-projection-ui/lib/section-workspaces";
import { SectionWorkspace } from "@erpx/platform-workplace/components/section-workspace";
import { type RouteSearch } from "@erpx/platform-shared-ui/lib/route-context";

import { ActionLink as UiActionLink } from "@erpx/platform-ui/components/foundation";
import { BetaSettings } from "@erpx/collaboration-ui/components/settings";
import { WidgetGrid } from "@erpx/platform-shared-ui/components/features/feature-page";
import { adminSections, type AdminSection } from "@erpx/platform-shared-ui/lib/administration";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { peopleContextHref } from "@erpx/people-payroll-ui/lib/people-route-context";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { administrationAccess } from "@erpx/platform-runtime-server/public/administration";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { accessPage } from "@erpx/work-projection-server/public/page-error";
import { notFound, redirect } from "next/navigation";
import styles from "@erpx/platform-workplace/routes/holdings/[holdingSlug]/_presenters/settings/settings.module.css";
export default async function Settings({ params, searchParams, }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<RouteSearch>;
}) {
    const actor = await requireWorkContext((await params).holdingSlug), base = workspaceBase(publicScope(actor));
    const access = await administrationAccess(actor), query = await searchParams;
    const section = ("usage") as AdminSection;
    if (!actor.beta && false)
        redirect(canonicalUiHref(`${base}/people?company=${encodeURIComponent(actor.companyId)}&tab=settings`));
    if (!actor.beta && false)
        redirect(peopleContextHref(base, query, { panel: "settings", section: null }));
    // Preserve the legacy staff-settings read-only organization view.
    const readOnlyOrganization = !actor.beta && false && !access.manageOrganization;
    if (!adminSections.some(([id]) => id === section))
        notFound();
    if (!access.sections.includes(section) && !readOnlyOrganization)
        return accessPage("permission", actor.holdingSlug);
    ;
    
    
    
    const canOpenReadiness = access.manageOrganization && !actor.branchId && !actor.departmentId;
    const canOpenStripe = !actor.branchId &&
        !actor.departmentId &&
        (hasPermission(actor, "finance.approve") ||
            hasPermission(actor, "pos.payments.configure"));
    const sharedTopics = sectionWorkspaceTopics.administration.filter(topic => access.sections.includes(topic.query.section as AdminSection) && (topic.id !== 'brand:media' || access.manageOrganization || access.owner));
    const topics = workspaceTopicLinks(base, query, sharedTopics);
    if (actor.beta)
        for (const [id, title] of adminSections.filter(([id]) => access.sections.includes(id) && ['people', 'organization'].includes(id)))
            topics.push({ id, title, description: undefined, href: canonicalUiHref(`${base}/settings?section=${id}`) });
    
    
    ;
    ;
    return (<SectionWorkspace title="Administration settings" items={topics} selected={section} context={adminSections.find(([id]) => id === section)?.[1]}>



      {(canOpenReadiness || canOpenStripe) && (<nav className={styles.shortcuts} aria-label="Related settings">
          {canOpenReadiness && (<UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/settings/business-readiness`)}>
              Business readiness · A–J
            </UiActionLink>)}
          {canOpenStripe && (<UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/settings/stripe`)}>
              Company Stripe · POS and PMS
            </UiActionLink>)}
        </nav>)}
      {false}
      {false}
      <WidgetGrid>
        {false}
        {false}
        {false}
        {false}
        {false}
        {(<BetaSettings key={section} base={base} owner={access.owner} section={section} beta={!!actor.beta}/>)}
      </WidgetGrid>
    </SectionWorkspace>);
}
