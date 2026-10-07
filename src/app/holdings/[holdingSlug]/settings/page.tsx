import { operationsPresentationQueries } from "@/server/modules/operations/public/presentation-queries";


import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { BetaSettings } from "@/components/beta/settings";
import {
  FeatureTabBar,
  FeaturePage,
  FeatureWidget,
  WidgetGrid,
} from "@/components/features/feature-page";
import { AiProviders } from "@/components/modules/ai-providers";
import { ApprovalAdministration } from "@/components/modules/approval-administration";
import { BrandAdministration } from "@/components/modules/brand-administration";
import { ServiceNavigation } from "@/components/modules/service-navigation";
import { StaffSettingsControls } from "@/components/modules/staff-settings-controls";
import { PageHeader } from "@/components/ui/primitives";
import { FlowSettingsHub } from "@/components/workplace/flow-settings-hub";
import { RequestAdministration } from "@/components/workplace/request-administration";
import { ServiceTeams } from "@/components/workplace/service-teams";
import { adminSections, type AdminSection } from "@/lib/administration";
import { workspaceBase } from "@/lib/workspace";
import { peopleContextHref } from "@/lib/people-route-context";
import { hasPermission } from "@/lib/permissions";
import { administrationAccess } from "@/server/modules/runtime/public/administration";

import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";

import { accessPage } from "@/server/modules/work_projection/public/page-error";



import { RecordLink as Link } from "@/components/records/record-link";
import { notFound, redirect } from "next/navigation";
import styles from "./settings.module.css";
import { Card } from "@/components/ui/foundation";
export default async function Settings({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ section?: string; kind?: string; record?: string; tab?: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor));
  const access = await administrationAccess(actor),
    query = await searchParams;
  const section = (query.section ??
    (access.managePeople || access.owner || access.manageAiProviders
      ? access.sections[0]
      : "feedback")) as AdminSection;
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
    (query.record || query.kind)
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
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <PageHeader appearance="standard"
        eyebrow={
          actor.beta || section === "feedback"
            ? undefined
            : section === "usage"
              ? "Technology"
              : section === "ai"
                ? "Technology"
                : "Projects & workflows"
        }
        title={
          section === "feedback"
            ? "Help & feedback"
            : actor.beta
              ? "Administration"
              : section === "approvals"
                ? access.manageOrganization
                  ? "Approval routing"
                  : "Routine templates"
                : adminSections.find(([id]) => id === section)![1]
        }
        description={
          section === "feedback"
            ? "Get help or tell us what could work better."
            : actor.beta
              ? "Manage your workspace, one place at a time."
              : section === "approvals"
                ? "Review routes, resolve blocked work, and inspect recorded configuration changes."
                : section === "usage"
                  ? "Manage company usage and billing."
                  : "Configure AI providers for your company."
        }
      />
      {!actor.beta && section !== "feedback" && (
        <ServiceNavigation
          context={{
            workspaceBase: base,
            scopeKind: actor.scope.kind,
            role: actor.role,
            permissions: actor.iam?.permissions,
            adminSections: access.sections,
            canManageApprovals: access.manageOrganization,
          }}
          groupId={
            section === "usage"
              ? "technology"
              : section === "ai"
                ? "technology"
                : "projects"
          }
          activeId={
            section === "approvals" && !access.manageOrganization
              ? "projects-routines"
              : `administration-${section}`
          }
        />
      )}
      {actor.beta && section !== "feedback" && access.sections.length > 1 && (
        <FeatureTabBar appearance="standard" aria-label="Administration sections">
          {adminSections
            .filter(
              ([id]) =>
                access.sections.includes(id) &&
                id !== "feedback" &&
                (actor.beta || !["people", "organization"].includes(id)),
            )
            .map(([id, label]) => (
              <Link
                key={id}
                href={`${base}/settings?section=${id}`}
                aria-current={id === section ? "page" : undefined}
              >
                {label}
              </Link>
            ))}
        </FeatureTabBar>
      )}
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
      {section === "approvals" && !query.record && <FlowSettingsHub actor={actor} base={base} tab={["templates","issues","history"].includes(query.tab??"")?query.tab!:"templates"}/>}
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
        {section === "approvals" && (query.record || query.tab === "issues" || !access.manageOrganization) &&
          (access.manageOrganization ? (
            query.kind === "request" ? (
              <RequestAdministration
                actor={actor}
                base={base}
                record={query.record}
              />
            ) : (
              <ApprovalAdministration
                actor={actor}
                base={base}
                kind={query.kind}
                record={query.record}
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
        {section === "approvals" && access.owner && !query.record && query.tab === "issues" && (
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
    </FeaturePage>
  );
}
