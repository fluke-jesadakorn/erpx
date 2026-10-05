import { RecordLink as Link } from "@/components/records/record-link";
import { OrganizationAccessWorkspace } from "@/components/technology/organization-access-workspace";
import { organizationAccessData } from "@/server/organization-access";
import { gpoRulesOverview } from "@/server/gpo-rules";
import { PageHeader } from "@/components/ui/primitives";
import {
  AccessPackages,
  PreparationTasks,
} from "@/components/people/lifecycle-workspace";
import { UserManagement } from "@/components/people/user-management";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { iamContext } from "@/server/iam-context";
import { peopleLifecycleData } from "@/server/people-lifecycle";
import { directoryAutomationData } from "@/server/directory-operations";
import { technologyData } from "@/server/technology";
import {
  peopleAccessData,
  accessMetadata,
  accessEntries,
} from "@/server/people-access";
import { readManagement } from "@/server/user-management";
import { workspaceBase, workspaceApiHref } from "@/lib/workspace";
import { hasPermission } from "@/lib/permissions";
import { pageError } from "@/server/page-error";
import styles from "@/components/technology/technology.module.css";
const views = [
  ["organization", "Organization & Access"],
  ["resources", "Resources"],
  ["attributes", "Custom attributes"],
] as const;
export default async function TechnologyPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ view?: string; position?: string; employee?: string; person?: string; level?: string; department?: string; branch?: string; object?: string; connection?: string; section?: string; unassigned?: string }>;
}) {
  const { holdingSlug } = await params,
    query = await searchParams;
  const localContext = await requireWorkContext(holdingSlug);
  if (query.view !== "resources" && query.view !== "attributes") {
    const resolved = await (async () => { try {
      const { db, actor } = await iamContext(holdingSlug);
      const canReadDirectory = hasPermission(actor, "iam.read") && !localContext.branchId && !localContext.departmentId;
      const [organization, lifecycle, automation, technology, gpoRules] = await Promise.all([
        organizationAccessData(db, localContext),
        peopleLifecycleData(db, localContext),
        canReadDirectory ? directoryAutomationData(db, actor) : Promise.resolve({ cases: [], connections: [], jobs: [], canApply: false, canManage: false, canReview: false }),
        canReadDirectory ? technologyData(db, actor) : Promise.resolve({ applications: [], operations: [] }),
        gpoRulesOverview(db, localContext),
      ]);
      return { organization, lifecycle, automation, technology, gpoRules, actor, canReadDirectory };
    } catch (error) { return pageError(error, holdingSlug); } })();
    const { organization, lifecycle, automation, technology, gpoRules, actor, canReadDirectory } = resolved;
    const base = workspaceBase(publicScope(localContext));
      const initialSelection: Parameters<typeof OrganizationAccessWorkspace>[0]["initialSelection"] = query.position ? { kind: "position", id: query.position }
        : query.employee || query.person ? { kind: "employee", id: query.employee ?? organization.people.find(person => person.person_id === query.person)?.engagement_id ?? query.person! }
        : query.department ? { kind: "department", id: query.department }
        : query.level ? { kind: "level", id: query.level }
        : query.branch ? { kind: "branch", id: query.branch }
        : query.object ? { kind: "directory", id: query.object, connectionId: query.connection }
        : query.unassigned ? { kind: "unassigned", id: "unassigned" }
        : query.view === "directory" ? { kind: "directory", id: "directory" }
        : query.view === "employees" && organization.people.length ? { kind: "employee", id: organization.people[0].engagement_id }
        : { kind: "company", id: "company" };
      return <div className={`content-stack ${styles.consolePage}`} data-source="live">
        <PageHeader appearance="standard" eyebrow="Technology" title="Organization & Access" description="Organize people, positions, and policies in one workspace." />
        <OrganizationAccessWorkspace base={base} companyName={organization.nodes.find(node => node.kind === "company")?.display_name ?? localContext.holdingName} organization={organization} lifecycle={lifecycle} automation={automation} technology={technology} gpoRules={gpoRules} canApply={canReadDirectory && hasPermission(actor, "iam.directory.apply")} initialSelection={initialSelection} initialSection={query.section} />
      </div>;
  }
  if (query.view === "attributes") {
    const { db, actor } = await iamContext(holdingSlug);
    const [access, management] = await Promise.all([
      peopleAccessData(db, actor),
      readManagement(db, actor),
    ]);
    const scope = publicScope(localContext),
      base = workspaceBase(scope),
      meta = accessMetadata(access);
    return <div className={`content-stack ${styles.consolePage}`} data-source="live">
      <PageHeader appearance="standard" eyebrow="Technology" title="Custom attributes" description="Define additional fields for people, groups, and organizational units." />
      <nav className={styles.tabs} data-feature-tabs aria-label="Technology views">{views.map(([id, name]) => <Link key={id} href={`${base}/technology?view=${id}`} aria-current={id === "attributes" ? "page" : undefined}>{name}</Link>)}</nav>
      <UserManagement compactConsole initial={management} initialView="attributes" views={["attributes"]} roles={meta.roles} base={base} api={workspaceApiHref(scope, "people/access")} accounts={{ meta, initialEntries: accessEntries(access, new URLSearchParams()) }} />
    </div>;
  }
  const resolved = await (async () => {
    try {
      const { db, actor } = await iamContext(holdingSlug),
        context = await requireWorkContext(holdingSlug);
      const [lifecycle, access, management] =
        await Promise.all([
          peopleLifecycleData(db, context),
          peopleAccessData(db, actor),
          readManagement(db, actor),
        ]);
      return {
        actor,
        context,
        lifecycle,
        access,
        management,
      };
    } catch (e) {
      return pageError(e, holdingSlug);
    }
  })();
  const { lifecycle, access, management } =
      resolved,
    scope = publicScope(resolved.context),
    base = workspaceBase(scope),
    view = "resources",
    resourceSection = (["packages", "accounts", "groups", "policies", "tasks"] as const).find((item) => item === query.section) ?? "packages",
    meta = accessMetadata(access);
  return (
    <div className={`content-stack ${styles.consolePage}`} data-source="live">
      <PageHeader appearance="standard"
        eyebrow="Technology"
        title="Resources"
        description="Create packages, manage access policies, and track preparation tasks."
      />
      <nav className={styles.tabs} data-feature-tabs aria-label="Technology views">
        {views.map(([id, name]) => (
          <Link
            key={id}
            href={`${base}/technology?view=${id}`}
            aria-current={view === id ? "page" : undefined}
          >
            {name}
          </Link>
        ))}
        <Link href={`${base}/people/settings?section=connections`}>
          Connections
        </Link>
        <Link href={`${base}/people/settings?section=reviews`}>
          Reviews & audit
        </Link>
      </nav>
      {view === "resources" && <nav className={styles.sectionTabs} aria-label="Resource sections">
        {([
          ["packages", "Packages"],
          ["accounts", "Accounts"],
          ["groups", "Groups"],
          ["policies", "ERPX policies"],
          ["tasks", "Tasks"],
        ] as const).map(([id, label]) => <Link key={id} href={`${base}/technology?view=resources&section=${id}`} aria-current={resourceSection === id ? "page" : undefined}>{label}</Link>)}
      </nav>}
      {view === "resources" && resourceSection === "packages" && (
          <AccessPackages
            base={base}
            data={lifecycle}
            roles={access.overview.roles}
            groups={management.groups}
            connections={access.overview.connections
              .filter((c) => c.status === "active")
              .map((c) => ({
                id: String(c.id),
                name: String(c.name),
                provider: String(c.provider),
              }))}
            directoryGroups={access.overview.groups.map((g) => ({
              id: String(g.external_id),
              name: String(g.name),
              connectionId: String(g.connection_id),
            }))}
          />
      )}
      {view === "resources" && (["accounts", "groups", "policies"] as const).some((item) => item === resourceSection) && (
            <UserManagement
              key={resourceSection}
              compactConsole
              initial={management}
              initialView={resourceSection === "accounts" ? "people" : resourceSection === "groups" ? "groups" : "policies"}
              views={[resourceSection === "accounts" ? "people" : resourceSection === "groups" ? "groups" : "policies"]}
              roles={meta.roles}
              base={base}
              api={workspaceApiHref(scope, "people/access")}
              accounts={{
                meta,
                initialEntries: accessEntries(access, new URLSearchParams()),
              }}
            />
      )}
      {view === "resources" && resourceSection === "tasks" && <PreparationTasks base={base} data={lifecycle} />}
    </div>
  );
}
