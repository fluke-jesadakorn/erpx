import { ServiceTeams } from "@/components/workplace/service-teams";
import { RequestAdministration } from "@/components/workplace/request-administration";
import { serviceTeamOptions } from "@/server/service-team-options";
import { hasBusinessRole } from "@/lib/responsibilities";
import Link from "next/link";
import { AccountInvitations } from "@/components/modules/account-invitations";
import { notFound } from "next/navigation";
import { BetaSettings } from "@/components/beta/settings";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { runtimeEnv, holdingResources } from "@/server/runtime";
import { aiProviderOverview } from "@/server/ai-providers";
import { AiProviders } from "@/components/modules/ai-providers";
import { workspaceBase } from "@/lib/workspace";
import { administrationAccess } from "@/server/administration";
import { adminSections, type AdminSection } from "@/lib/administration";
import { staffSettings } from "@/server/staff-settings";
import { StaffSettingsControls } from "@/components/modules/staff-settings-controls";
import { ApprovalAdministration } from "@/components/modules/approval-administration";
import { PageHeader } from "@/components/ui/primitives";
import styles from "@/components/workplace/pages.module.css";
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
  // Preserve the legacy staff-settings read-only organization view.
  const readOnlyOrganization =
    !actor.beta && section === "organization" && !access.manageOrganization;
  if (!access.sections.includes(section) && !readOnlyOrganization) notFound();
  if (
    section === "approvals" &&
    !access.manageOrganization &&
    (query.record || query.kind)
  )
    notFound();
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
    <main className="content-stack" data-source="live">
      <PageHeader
        title={
          access.sections.length > 1 ? "Administration" : "Help & feedback"
        }
        description="Manage your workspace, one place at a time."
      />
      {access.sections.length > 1 && (
        <nav className={styles.tabs} aria-label="Administration sections">
          {adminSections
            .filter(([id]) => access.sections.includes(id))
            .map(([id, label]) => (
              <Link
                key={id}
                href={`${base}/settings?section=${id}`}
                aria-current={id === section ? "page" : undefined}
              >
                {label}
              </Link>
            ))}
        </nav>
      )}
      {section === "people" && (
        <section className="card">
          <h2>People & access</h2>
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
        </section>
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
        hasBusinessRole(actor, ["holding_owner"]) && (
          <AccountInvitations actor={actor} base={base} />
        )}
      {section === "organization" && options && (
        <>
          {access.owner && <ServiceTeams base={base} options={await serviceTeamOptions(write, actor)} companyId={actor.companyId} userId={actor.userId}/>}
          <section className="card">
            <h2>Branches</h2>
            {branches?.results.map((b) => (
              <p key={b.display_name}>{b.display_name}</p>
            ))}
          </section>
          {access.manageOrganization ? (
            <StaffSettingsControls base={base} options={options} />
          ) : (
            <section className="card">
              <h2>Staff levels and department heads</h2>
              {options.levels.map((l) => (
                <p key={l.id}>
                  {l.name} · rank {l.rank}
                </p>
              ))}
              {options.departments.map((d) => (
                <p key={d.id}>
                  {d.display_name} · {d.head_name}
                </p>
              ))}
            </section>
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
          <section className="card">
            <h2>Routines</h2>
            <p>Browse published routines and manage the templates you own.</p>
            <Link className="button" href={`${base}/routine-templates`}>
              Routine templates
            </Link>
          </section>
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
    </main>
  );
}
