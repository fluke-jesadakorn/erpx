import { iamPresentationQueries } from "@/server/modules/identity_access/public/presentation-context-queries";
import { peoplePayrollPresentationQueries } from "@/server/modules/people_payroll/public/presentation-queries";


import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { RecordLink as Link } from "@/components/records/record-link";
import { notFound, redirect } from "next/navigation";
import { FeaturePage, FeatureWidget, WidgetGrid } from "@/components/features/feature-page";
import { IamConsole } from "@/components/iam/iam-console";
import { OrganizationWorkspace } from "@/components/modules/organization-workspace";
import { AccountInvitations } from "@/components/modules/account-invitations";
import { PeopleNavigation } from "@/components/modules/people-navigation";
import { LevelLeaveDefaults } from "@/components/people/level-leave-defaults";

import { ServiceNavigation } from "@/components/modules/service-navigation";
import { DirectoryWorkspace } from "@/components/people/directory-workspace";
import { HoldingTemplates } from "@/components/people/holding-templates";
import { LifecycleSettings } from "@/components/people/lifecycle-workspace";
import { SelfServiceSettings } from "@/components/self-service/settings";
import { PageHeader } from "@/components/ui/primitives";
import { iamSections, type IamSection } from "@/lib/iam";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { peopleContextHref, type PeopleRouteSearch } from "@/lib/people-route-context";
import { administrationAccess } from "@/server/modules/runtime/public/administration";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";

import { ApiError } from "@/server/modules/runtime/public/http";


import { leaveAdministration } from "@/server/modules/people_payroll/public/leave-self-service";





import styles from "./settings.module.css";

const sections = [
  ["", "Overview"],
  ["leave", "Leave & overtime"],
  ["templates", "HR templates"],
] as const;

export default async function PeopleSettings({ params, searchParams }: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<PeopleRouteSearch>;
}) {
  const slug = (await params).holdingSlug;
  const actor = await requireWorkContext(slug);
  const base = workspaceBase(publicScope(actor));
  const query = await searchParams;
  const section = typeof query.section === "string" ? query.section : "";
  if (!["", "levels", "leave", "calendars", "pay-groups", "templates", "lifecycle", "attributes", ...iamSections.map(([id]) => id)].includes(section)) notFound();
  if (section === "attributes") redirect(`${base}/technology?view=attributes`);
  const technology = !actor.beta && ["connections", "roles", "matrix", "reviews", "groups"].includes(section);
  if (!actor.beta && ["", "levels", "leave", "calendars", "pay-groups", "templates", "lifecycle"].includes(section)) redirect(peopleContextHref(base, query, { panel: "settings" }));
  const feature = section === "matrix" ? "roles" : section;
  const featureTitle = feature === "groups" ? "Groups & policies" : (iamSections.find(([id]) => id === feature)?.[1] ?? "People settings");
  const admin = technology ? await administrationAccess(actor) : null;
  const presentation = await peoplePayrollPresentationQueries(actor.holdingId);
  const needsIam = hasPermission(actor, "iam.read") && (section === "templates" || iamSections.some(([id]) => id === section));
  const iam = needsIam || section === "my-access" ? await iamPresentationQueries(slug) : null;
  const data = iam && (section === "templates" || iamSections.some(([id]) => id === section)) ? await iam.queries.iamOverview(iam.actor, section === "my-access") : null;
  const management = iam && section === "connections" ? await iam.queries.readManagement(iam.actor) : null;
  const canManageTemplates = hasPermission(actor, "organization.manage") && hasPermission(actor, "people.manage");
  const lifecycle = canManageTemplates && (section === "templates" || section === "") ? await presentation.peopleLifecycleData(actor) : null;
  const holdingTemplates = section === "templates" && iam?.actor.scopeKind === "holding" && query.view !== "advanced" ? await iam.queries.peopleTemplatesData(iam.actor) : null;
  let leave: Awaited<ReturnType<typeof leaveAdministration>> | null = null;
  if (section === "leave" || section === "") {
    try { leave = await leaveAdministration(actor); }
    catch (error) { if (!(error instanceof ApiError && error.status === 403)) throw error; }
  }
  const templateOwners = data?.users.filter((user) => data.assignments.some((a) => a.user_id === user.id && a.active && a.scope_kind === "holding" && data.roles.some((role) => role.id === a.role && role.permissions.includes("iam.directory.manage")))) ?? [];

  return <FeaturePage className="content-stack" data-source="live">
    <PageHeader appearance="standard" eyebrow={technology ? "Technology" : "People"} title={technology ? featureTitle : "People settings"} description={technology ? "Manage directory connections, security, and access configuration." : "Choose a settings area to review status and complete HR setup."} />
    {technology ? <ServiceNavigation context={{ workspaceBase: base, scopeKind: actor.scope.kind, role: actor.role, permissions: actor.iam?.permissions, adminSections: admin?.sections }} groupId="technology" activeId={`technology-${feature}`} /> : <PeopleNavigation base={base} actor={actor} scopeKind={actor.scope.kind} active="settings" />}
    {!technology && ["", "leave", "templates"].includes(section) && <nav className={styles.sections} aria-label="People settings sections">{sections.filter(([id]) => id !== "leave" || !!leave).filter(([id]) => id !== "templates" || canManageTemplates).map(([id, label]) => <Link key={id} href={id ? `${base}/people/settings?section=${id}` : `${base}/people/settings`} aria-current={section === id ? "page" : undefined}>{label}</Link>)}</nav>}
    {section === "" && <div className={styles.overview}>
      <p className={styles.intro}>Select an area to see its records and actions. Status is based on your current Company settings.</p>
      {leave && <Link className={styles.summary} href={`${base}/people/settings?section=leave`}><span><strong>Leave & overtime</strong><small>{leave.types.length} leave types · {leave.policies.length} policy versions · {leave.assignments.filter(a => !a.effective_to).length} active assignments</small></span><span>Open settings →</span></Link>}
      {canManageTemplates && lifecycle && <Link className={styles.summary} href={`${base}/people/settings?section=templates`}><span><strong>HR templates</strong><small>IT handoff owner: {lifecycle.settings.itAssigneeId ? "Assigned" : "Unassigned"} · {lifecycle.tasks.filter(t => t.kind === "position" && t.status === "pending").length} position preparation tasks pending</small></span><span>Open templates →</span></Link>}
      {!leave && !canManageTemplates && <p>Your selected role does not manage these settings.</p>}
    </div>}
    {section === "leave" && (leave ? <><SelfServiceSettings base={base} data={leave} /><details className={styles.advancedSettings}><summary>Leave defaults by staff level</summary><LevelLeaveDefaults appearance="standard" base={base} levels={(await presentation.leaveDefaultLevels(actor.companyId)).results} defaults={(await presentation.defaultCatalog(actor)).leave}/></details></> : <p>HR setup or permission is required to manage leave and overtime.</p>)}
    {section === "templates" && canManageTemplates && (query.view === "advanced" ? <><UiActionLink appearance="standard" variant="secondary" href={`${base}/people/settings?section=templates`}>← Back to HR templates</UiActionLink><OrganizationWorkspace appearance="standard" base={base} data={await presentation.organizationOverview(actor)} initialView="templates" /></> : <WidgetGrid><FeatureWidget appearance="standard" id="templates" title="Templates & lifecycle handoff"><div className={styles.templateStack}>{lifecycle && <LifecycleSettings base={base} data={lifecycle} owners={templateOwners} />}{holdingTemplates && <HoldingTemplates base={base} data={holdingTemplates} />}<Link href={`${base}/people/settings?section=templates&view=advanced`}>Advanced organization templates →</Link></div></FeatureWidget></WidgetGrid>)}
    {iam && data && iamSections.some(([id]) => id === section) && <WidgetGrid>{(feature === "roles" ? (["roles", "matrix"] as IamSection[]) : [section as IamSection]).map(panel => <FeatureWidget appearance="standard" key={panel} id={panel} title={iamSections.find(([id]) => id === panel)?.[1] ?? panel}><IamConsole embedded data={data} base={base} api={`/api/v1${base}/iam`} section={panel} /></FeatureWidget>)}{section === "reviews" && hasPermission(actor, "iam.roles.manage") && <FeatureWidget appearance="standard" id="account-invitations" title="Activation invitations"><AccountInvitations actor={actor} base={base} /></FeatureWidget>}</WidgetGrid>}
    {section === "connections" && iam?.actor.scopeKind === "holding" && <DirectoryWorkspace appearance="standard" base={base} initial={await iam.queries.directoryAutomationData(iam.actor)} mode="settings" attributes={management?.attributes ?? []} people={management?.people ?? []} />}
  </FeaturePage>;
}
