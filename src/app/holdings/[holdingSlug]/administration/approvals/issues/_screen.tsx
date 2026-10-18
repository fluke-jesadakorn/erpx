import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@/lib/work_projection/section-workspaces";
import { SectionWorkspace } from "@/components/workplace/section-workspace";
import { type RouteSearch } from "@/lib/shared/route-context";

import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { BetaSettings } from "@/components/collaboration/settings";
import { FeatureWidget, WidgetGrid } from "@/components/shared/features/feature-page";
import { ApprovalAdministration } from "@/components/workflow/approval-administration";
import { FlowSettingsHub } from "@/components/workplace/flow-settings-hub";
import { RequestAdministration } from "@/components/workplace/request-administration";
import { adminSections, type AdminSection } from "@/lib/shared/administration";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { peopleContextHref } from "@/lib/people_payroll/people-route-context";
import { hasPermission } from "@/lib/identity_access/permissions";
import { administrationAccess } from "@/server/modules/runtime/public/administration";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { accessPage } from "@/server/modules/work_projection/public/page-error";
import { notFound, redirect } from "next/navigation";
import styles from "@/app/holdings/[holdingSlug]/_presenters/settings/settings.module.css";
import { Card } from "@/components/ui/foundation";
export default async function Settings({ params, searchParams, }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<RouteSearch>;
}) {
    const actor = await requireWorkContext((await params).holdingSlug), base = workspaceBase(publicScope(actor));
    const access = await administrationAccess(actor), query = await searchParams;
    const section = ("approvals") as AdminSection;
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
    if (!access.manageOrganization &&
        ((typeof query.record === "string" ? query.record : undefined) || (typeof query.kind === "string" ? query.kind : undefined)))
        return accessPage("permission", actor.holdingSlug);
    
    
    
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
    const approvalTab = "issues";
    
    ;
    ;
    return (<SectionWorkspace title="Administration settings" items={topics} selected={`approvals:${approvalTab}`} context={adminSections.find(([id]) => id === section)?.[1]}>



      {(canOpenReadiness || canOpenStripe) && (<nav className={styles.shortcuts} aria-label="Related settings">
          {canOpenReadiness && (<UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/settings/business-readiness`)}>
              Business readiness · A–J
            </UiActionLink>)}
          {canOpenStripe && (<UiActionLink appearance="standard" variant="secondary" href={canonicalUiHref(`${base}/settings/stripe`)}>
              Company Stripe · POS and PMS
            </UiActionLink>)}
        </nav>)}
      {!(typeof query.record === "string" ? query.record : undefined) && <FlowSettingsHub embedded actor={actor} base={base} tab={"issues"}/>}
      {false}
      <WidgetGrid>
        {false}
        {false}
        {false}
        {((typeof query.record === "string" ? query.record : undefined) || true || !access.manageOrganization) &&
            (access.manageOrganization ? ((typeof query.kind === "string" ? query.kind : undefined) === "request" ? (<RequestAdministration actor={actor} base={base} record={(typeof query.record === "string" ? query.record : undefined)}/>) : (<ApprovalAdministration actor={actor} base={base} kind={(typeof query.kind === "string" ? query.kind : undefined)} record={(typeof query.record === "string" ? query.record : undefined)}/>)) : (<FeatureWidget appearance="standard" id="people-settings-routines" title="Routines">
              <Card as="section" appearance="standard" className="ui-panel">
                <p>
                  Browse published routines and manage the templates you own.
                </p>
                <UiActionLink appearance="standard" variant="primary" href={canonicalUiHref(`${base}/routine-templates`)}>
                  Routine templates
                </UiActionLink>
              </Card>
            </FeatureWidget>))}
        {access.owner && !(typeof query.record === "string" ? query.record : undefined) && true && (<RequestAdministration actor={actor} base={base}/>)}
        {((actor.beta && ["people", "organization"].includes(section))) && (<BetaSettings key={section} base={base} owner={access.owner} section={section} beta={!!actor.beta}/>)}
      </WidgetGrid>
    </SectionWorkspace>);
}
