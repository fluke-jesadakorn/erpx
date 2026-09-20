import { BetaSettings } from "@/components/beta/settings";
import { FeatureTabBar, FeaturePage, FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { AccountInvitations } from "@/components/modules/account-invitations";
import { AiProviders } from "@/components/modules/ai-providers";
import { ApprovalAdministration } from "@/components/modules/approval-administration";
import { BrandAdministration } from "@/components/modules/brand-administration";
import { ServiceNavigation } from "@/components/modules/service-navigation";
import { StaffSettingsControls } from "@/components/modules/staff-settings-controls";
import { PageHeader } from "@/components/ui/primitives";
import styles from "@/components/workplace/pages.module.css";
import { RequestAdministration } from "@/components/workplace/request-administration";
import { ServiceTeams } from "@/components/workplace/service-teams";
import { adminSections,type AdminSection } from "@/lib/administration";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { administrationAccess } from "@/server/administration";
import { aiProviderOverview } from "@/server/ai-providers";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { listBrandProfiles } from "@/server/branding";
import { accessPage } from "@/server/page-error";
import { holdingResources,runtimeEnv } from "@/server/runtime";
import { serviceTeamOptions } from "@/server/service-team-options";
import { staffSettings } from "@/server/staff-settings";
import Link from "next/link";
import { notFound,redirect } from "next/navigation";
export default async function Settings({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ section?: string; kind?: string; record?: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor));
  const access = await administrationAccess(actor),
    query = await searchParams;
  const section = (query.section ??
    (access.managePeople || access.owner || access.manageAiProviders
      ? access.sections[0]
      : "feedback")) as AdminSection;
  if(!actor.beta && section === "organization")redirect(`${base}/people/organization`);
  if(!actor.beta && section === "people")redirect(`${base}/people/settings`);
  // Preserve the legacy staff-settings read-only organization view.
  const readOnlyOrganization =
    !actor.beta && section === "organization" && !access.manageOrganization;
  if (!adminSections.some(([id]) => id === section)) notFound();
  if (!access.sections.includes(section) && !readOnlyOrganization) return accessPage("permission", actor.holdingSlug);
  if (
    section === "approvals" &&
    !access.manageOrganization &&
    (query.record || query.kind)
  )
    return accessPage("permission", actor.holdingSlug);
  const { write } = await holdingResources(actor.holdingId);
  const options =
    section === "organization" ? await staffSettings(write, actor) : null;
  const branches = options
    ? await write
        .prepare(
          "SELECT display_name FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name",
        )
        .bind(actor.companyId, actor.branchId, actor.branchId)
        .all<{ display_name: string }>()
    : null;
  return (
    <FeaturePage className="content-stack" data-source="live">
      <PageHeader
        eyebrow={actor.beta || section === "feedback" ? undefined : section === "usage" ? "Finance" : section === "ai" ? "Technology" : "Projects & workflows"}
        title={
          section === "feedback" ? "Help & feedback" : actor.beta ? "Administration" : section === "approvals" ? access.manageOrganization ? "Approvals" : "Routine templates" : adminSections.find(([id]) => id === section)![1]
        }
        description={section === "feedback" ? "Get help or tell us what could work better." : actor.beta ? "Manage your workspace, one place at a time." : section === "approvals" ? "Review approvals and manage routines." : section === "usage" ? "Manage company usage and billing." : "Configure AI providers for your company."}
      />
      {!actor.beta && section !== "feedback" && <ServiceNavigation
        context={{ workspaceBase: base, scopeKind: actor.scope.kind, role: actor.role, permissions: actor.iam?.permissions, adminSections: access.sections, canManageApprovals: access.manageOrganization }}
        groupId={section === "usage" ? "finance" : section === "ai" ? "technology" : "projects"}
        activeId={section === "approvals" && !access.manageOrganization ? "projects-routines" : `administration-${section}`}
      />}
      {actor.beta && section !== "feedback" && access.sections.length > 1 && (
        <FeatureTabBar aria-label="Administration sections">
          {adminSections
            .filter(([id]) => access.sections.includes(id) && id !== "feedback" && (actor.beta || !["people", "organization"].includes(id)))
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
      {access.manageOrganization&&!actor.branchId&&!actor.departmentId&&<Link className="button secondary" href={`${base}/settings/business-readiness`}>Business readiness · A–J</Link>}
      <WidgetGrid>{section === "people" && (
        <FeatureWidget id="people-settings-people-access" title="People & access"><section className="card">
          <Link className="button secondary" href={`${base}/iam`}>Identity & access administration</Link>
          <p>
            Manage employee details and individual access from each person’s
            profile.
          </p>
          <Link className="button" href={`${base}/people`}>
            Open people directory
          </Link>
          {!actor.beta && (
            <p className="muted">
              To invite someone, open their profile and choose Invite to ERPX.
              Activation links must be shared privately; this flow does not send
              email.
            </p>
          )}
        </section></FeatureWidget>
      )}
      {section === "ai" && access.manageAiProviders && (
        <AiProviders
          base={base}
          canBilling={access.owner}
          data={await aiProviderOverview(write, actor, await runtimeEnv())}
        />
      )}
      {section === "people" &&
        !actor.beta &&
        hasPermission(actor,"iam.roles.manage") && (
          <AccountInvitations actor={actor} base={base} />
        )}
      {section === "organization" && options && (
        <>
          <BrandAdministration
            base={base}
            {...await listBrandProfiles(write, actor)}
            canHolding={access.owner}
            canBranches={access.manageOrganization}
          />
          <FeatureWidget id="people-settings-organization-chart-and-positions" title="Organization chart and positions"><section className="card"><p>Manage staff levels, reporting managers, vacancies, and reusable organization templates.</p><Link className="button" href={`${base}/people/organization`}>Open organization</Link></section></FeatureWidget>
          {access.owner && <ServiceTeams base={base} options={await serviceTeamOptions(write, actor)} companyId={actor.companyId} userId={actor.userId}/>}
          <FeatureWidget id="people-settings-branches" title="Branches"><section className="card">
            
            {branches?.results.map((b) => (
              <p key={b.display_name}>{b.display_name}</p>
            ))}
          </section></FeatureWidget>
          {access.manageOrganization ? (
            <StaffSettingsControls base={base} options={options} />
          ) : (
            <FeatureWidget id="people-settings-staff-levels-and-department-heads" title="Staff levels and department heads"><section className="card">
              
              {options.levels.map((l) => (
                <p key={l.id}>
                  {l.name} · {l.level_number?`Level ${l.level_number}`:"Legacy level"}
                </p>
              ))}
              {options.departments.map((d) => (
                <p key={d.id}>
                  {d.display_name} · {d.head_name}
                </p>
              ))}
            </section></FeatureWidget>
          )}
        </>
      )}
      {section === "approvals" &&
        (access.manageOrganization ? (
          query.kind === "request" ? <RequestAdministration actor={actor} base={base} record={query.record}/> : <ApprovalAdministration
            actor={actor}
            base={base}
            kind={query.kind}
            record={query.record}
          />
        ) : (
          <FeatureWidget id="people-settings-routines" title="Routines"><section className="card">
            
            <p>Browse published routines and manage the templates you own.</p>
            <Link className="button" href={`${base}/routine-templates`}>
              Routine templates
            </Link>
          </section></FeatureWidget>
        ))}
      {section === "approvals" && access.owner && !query.record && <RequestAdministration actor={actor} base={base}/>}
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
    </WidgetGrid></FeaturePage>
  );
}
