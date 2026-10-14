import {sectionWorkspaceTopics,workspaceTopicLinks} from "@/lib/work_projection/section-workspaces";
import {SectionWorkspace} from "@/components/workplace/section-workspace";
import {BrandIdentityConsole} from "@/components/brand/brand-identity-console";
import {brandIdentityWorkspace} from "@/server/modules/brand/public/brand-identity";
import {type RouteSearch} from "@/lib/shared/route-context";
import { operationsPresentationQueries } from "@/server/modules/operations/public/presentation-queries";


import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { BetaSettings } from "@/components/collaboration/settings";
import {
  FeatureWidget,
  WidgetGrid,
} from "@/components/shared/features/feature-page";
import { AiProviders } from "@/components/platform_administration/ai-providers";
import { ApprovalAdministration } from "@/components/workflow/approval-administration";
import { BrandAdministration } from "@/components/brand/brand-administration";
import { StaffSettingsControls } from "@/components/people_payroll/staff-settings-controls";
import { FlowSettingsHub } from "@/components/workplace/flow-settings-hub";
import { RequestAdministration } from "@/components/workplace/request-administration";
import { ServiceTeams } from "@/components/workplace/service-teams";
import { adminSections, type AdminSection } from "@/lib/shared/administration";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { peopleContextHref } from "@/lib/people_payroll/people-route-context";
import { hasPermission } from "@/lib/identity_access/permissions";
import { administrationAccess } from "@/server/modules/runtime/public/administration";

import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";

import { accessPage } from "@/server/modules/work_projection/public/page-error";



import { notFound, redirect } from "next/navigation";
import styles from "./settings.module.css";
import { Card } from "@/components/ui/foundation";
export default async function Settings({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<RouteSearch>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor));
  const access = await administrationAccess(actor),
    query = await searchParams;
  const section = (typeof query.section === "string" ? query.section : (actor.beta?access.sections[0]:sectionWorkspaceTopics.administration.find(topic=>access.sections.includes(topic.query.section as AdminSection))?.query.section)??"feedback") as AdminSection;
  if (!actor.beta && section === "organization")
    redirect(`${base}/people?company=${encodeURIComponent(actor.companyId)}&tab=settings`);
  if (!actor.beta && section === "people") redirect(peopleContextHref(base, query, { panel: "settings", section: null }));
  // Preserve the legacy staff-settings read-only organization view.
  const readOnlyOrganization =
    !actor.beta && section === "organization" && !access.manageOrganization;
  if (!adminSections.some(([id]) => id === section)) notFound();
  if (!access.sections.includes(section) && !readOnlyOrganization)
    return accessPage("permission", actor.holdingSlug);
  if (
    section === "approvals" &&
    !access.manageOrganization &&
    ((typeof query.record==="string"?query.record:undefined) || (typeof query.kind==="string"?query.kind:undefined))
  )
    return accessPage("permission", actor.holdingSlug);
  const presentation = await operationsPresentationQueries(actor.holdingId);
  const options =
    section === "organization" ? await presentation.staffSettings(actor) : null;
  const branches = options
    ? await presentation.scopedBranchNames(actor.companyId, actor.branchId, actor.branchId)
    : null;
  const canOpenReadiness =
    access.manageOrganization && !actor.branchId && !actor.departmentId;
  const canOpenStripe =
    !actor.branchId &&
    !actor.departmentId &&
    (hasPermission(actor, "finance.approve") ||
      hasPermission(actor, "pos.payments.configure"));
  const sharedTopics=sectionWorkspaceTopics.administration.filter(topic=>access.sections.includes(topic.query.section as AdminSection)&&(topic.id!=='brand:media'||access.manageOrganization||access.owner));
  const topics=workspaceTopicLinks(base,query,sharedTopics);
  if(actor.beta)for(const [id,title] of adminSections.filter(([id])=>access.sections.includes(id)&&['people','organization'].includes(id)))topics.push({id,title,description:undefined,href:`${base}/settings?section=${id}`});
  const approvalTab=typeof query.tab==='string'&&['templates','issues','history'].includes(query.tab)?query.tab:'templates';
  const brandTab=typeof query.tab==='string'?query.tab:'identity';
  if(section==='brand'&&!['identity','media','history'].includes(brandTab))notFound();
  if(section==='brand'&&!topics.some(topic=>topic.id===`brand:${brandTab}`))return accessPage('permission',actor.holdingSlug);
  return (
    <SectionWorkspace title="Administration settings" items={topics} selected={section==='brand'?`brand:${brandTab}`:section==='approvals'?`approvals:${approvalTab}`:section} context={section==='brand'?topics.find(topic=>topic.id===`brand:${brandTab}`)?.title:adminSections.find(([id])=>id===section)?.[1]}>



      {(canOpenReadiness || canOpenStripe) && (
        <nav className={styles.shortcuts} aria-label="Related settings">
          {canOpenReadiness && (
            <UiActionLink appearance="standard" variant="secondary"

              href={`${base}/settings/business-readiness`}
            >
              Business readiness · A–J
            </UiActionLink>
          )}
          {canOpenStripe && (
            <UiActionLink appearance="standard" variant="secondary" href={`${base}/settings/stripe`}>
              Company Stripe · POS and PMS
            </UiActionLink>
          )}
        </nav>
      )}
      {section === "approvals" && !(typeof query.record==="string"?query.record:undefined) && <FlowSettingsHub embedded actor={actor} base={base} tab={typeof query.tab==="string"&&["templates","issues","history"].includes(query.tab)?query.tab:"templates"}/>}
      {section==='brand'&&(brandTab==='media'?<BrandAdministration appearance="standard" base={base} {...await presentation.listBrandProfiles(actor)} canHolding={access.owner} canBranches={access.manageOrganization} canStorefront={access.manageOrganization&&actor.scope.kind==='holding'} canReviewPublications={hasPermission(actor,'records.review')||hasPermission(actor,'commerce.approve')}/>:<BrandIdentityConsole base={base} data={await brandIdentityWorkspace(actor)} section={brandTab==='history'?'history':'identity'} embedded/>)}
      <WidgetGrid>
        {section === "people" && (
          <FeatureWidget
            id="people-settings-people-access"
            title="People & access"
          >
            <Card as="section" appearance="standard" className="ui-panel">
              <UiActionLink appearance="standard" variant="secondary" href={`${base}/iam`}>
                Identity & access administration
              </UiActionLink>
              <p>
                Manage employee details and individual access from each person’s
                profile.
              </p>
              <UiActionLink appearance="standard" variant="primary" href={`${base}/people`}>
                Open people directory
              </UiActionLink>
              {!actor.beta && (
                <p className="muted">
                  To invite someone, open their profile and choose Invite to
                  ERPX. An independent administrator approves the request before
                  ERPX sends the activation email. Delivery status and recovery
                  actions appear below.
                </p>
              )}
            </Card>
          </FeatureWidget>
        )}
        {section === "ai" && access.manageAiProviders && (
          <AiProviders
            base={base}
            canBilling={access.owner}
            data={await presentation.aiProviderOverview(actor)}
          />
        )}
        {section === "organization" && options && (
          <>
            <BrandAdministration appearance="standard"
              base={base}
              {...await presentation.listBrandProfiles(actor)}
              canHolding={access.owner}
              canBranches={access.manageOrganization}
              canStorefront={
                access.manageOrganization && actor.scope.kind === "holding"
              }
              canReviewPublications={
                hasPermission(actor, "records.review") ||
                hasPermission(actor, "commerce.approve")
              }
            />
            <FeatureWidget
              id="people-settings-organization-chart-and-positions"
              title="Organization chart and positions"
            >
              <Card as="section" appearance="standard" className="ui-panel">
                <p>
                  Manage staff levels, reporting managers, vacancies, and
                  reusable organization templates.
                </p>
                <UiActionLink appearance="standard" variant="primary" href={`${base}/people?company=${encodeURIComponent(actor.companyId)}&tab=settings`}>
                  Open organization
                </UiActionLink>
              </Card>
            </FeatureWidget>
            {access.owner && (
              <ServiceTeams
                base={base}
                options={await presentation.serviceTeamOptions(actor)}
                companyId={actor.companyId}
                userId={actor.userId}
              />
            )}
            <FeatureWidget appearance="standard" id="people-settings-branches" title="Branches">
              <Card as="section" appearance="standard" className="ui-panel">
                {branches?.results.map((b) => (
                  <p key={b.display_name}>{b.display_name}</p>
                ))}
              </Card>
            </FeatureWidget>
            {access.manageOrganization ? (
              <StaffSettingsControls base={base} options={options} />
            ) : (
              <FeatureWidget
                id="people-settings-staff-levels-and-department-heads"
                title="Staff levels and department heads"
              >
                <Card as="section" appearance="standard" className="ui-panel">
                  {options.levels.map((l) => (
                    <p key={l.id}>
                      {l.name} ·{" "}
                      {l.level_number
                        ? `Level ${l.level_number}`
                        : "Legacy level"}
                    </p>
                  ))}
                  {options.departments.map((d) => (
                    <p key={d.id}>
                      {d.display_name} · {d.head_name}
                    </p>
                  ))}
                </Card>
              </FeatureWidget>
            )}
          </>
        )}
        {section === "approvals" && ((typeof query.record==="string"?query.record:undefined) || query.tab === "issues" || !access.manageOrganization) &&
          (access.manageOrganization ? (
            (typeof query.kind==="string"?query.kind:undefined) === "request" ? (
              <RequestAdministration
                actor={actor}
                base={base}
                record={(typeof query.record==="string"?query.record:undefined)}
              />
            ) : (
              <ApprovalAdministration
                actor={actor}
                base={base}
                kind={(typeof query.kind==="string"?query.kind:undefined)}
                record={(typeof query.record==="string"?query.record:undefined)}
              />
            )
          ) : (
            <FeatureWidget appearance="standard" id="people-settings-routines" title="Routines">
              <Card as="section" appearance="standard" className="ui-panel">
                <p>
                  Browse published routines and manage the templates you own.
                </p>
                <UiActionLink appearance="standard" variant="primary" href={`${base}/routine-templates`}>
                  Routine templates
                </UiActionLink>
              </Card>
            </FeatureWidget>
          ))}
        {section === "approvals" && access.owner && !(typeof query.record==="string"?query.record:undefined) && query.tab === "issues" && (
          <RequestAdministration actor={actor} base={base} />
        )}
        {(section === "usage" ||
          section === "feedback" ||
          (actor.beta && ["people", "organization"].includes(section))) && (
          <BetaSettings
            key={section}
            base={base}
            owner={access.owner}
            section={section}
            beta={!!actor.beta}
          />
        )}
      </WidgetGrid>
    </SectionWorkspace>
  );
}
