import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@erpx/feature-work_projection/lib/section-workspaces";
import { SectionWorkspace } from "@erpx/workplace/components/section-workspace";
import { BrandIdentityConsole } from "@erpx/feature-brand/components/brand-identity-console";
import { brandIdentityWorkspace } from "@erpx/server-brand/public/brand-identity";
import { type RouteSearch } from "@erpx/feature-shared/lib/route-context";

import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { BetaSettings } from "@erpx/feature-collaboration/components/settings";
import { WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { adminSections, type AdminSection } from "@erpx/feature-shared/lib/administration";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { peopleContextHref } from "@erpx/feature-people_payroll/lib/people-route-context";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { administrationAccess } from "@erpx/server-runtime/public/administration";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { accessPage } from "@erpx/server-work_projection/public/page-error";
import { notFound, redirect } from "next/navigation";
import styles from "@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/settings/settings.module.css";
export default async function Settings({ params, searchParams, }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<RouteSearch>;
}) {
    const actor = await requireWorkContext((await params).holdingSlug), base = workspaceBase(publicScope(actor));
    const access = await administrationAccess(actor), query = await searchParams;
    const section = ("brand") as AdminSection;
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
    
    const brandTab = "identity";
    ;
    if (!topics.some(topic => topic.id === `brand:${brandTab}`))
        return accessPage('permission', actor.holdingSlug);
    return (<SectionWorkspace title="Administration settings" items={topics} selected={`brand:${brandTab}`} context={topics.find(topic => topic.id === `brand:${brandTab}`)?.title}>



      {(canOpenReadiness || canOpenStripe) && (<nav className={styles.shortcuts} aria-label="Related settings">
          {canOpenReadiness && (<UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/settings/business-readiness`)}>
              Business readiness · A–J
            </UiActionLink>)}
          {canOpenStripe && (<UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/settings/stripe`)}>
              Company Stripe · POS and PMS
            </UiActionLink>)}
        </nav>)}
      {false}
      {(<BrandIdentityConsole base={base} data={await brandIdentityWorkspace(actor)} section={'identity'} embedded/>)}
      <WidgetGrid>
        {false}
        {false}
        {false}
        {false}
        {false}
        {((actor.beta && ["people", "organization"].includes(section))) && (<BetaSettings key={section} base={base} owner={access.owner} section={section} beta={!!actor.beta}/>)}
      </WidgetGrid>
    </SectionWorkspace>);
}
