import Link from "next/link";
import { PageHeader } from "@/components/ui/primitives";
import { DirectoryWorkspace } from "@/components/people/directory-workspace";
import {
  AccessPackages,
  PositionPreparation,
  PreparationTasks,
} from "@/components/people/lifecycle-workspace";
import { ProvisioningWorkspace } from "@/components/technology/provisioning-workspace";
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
  ["positions", "Positions"],
  ["employees", "Employees"],
  ["directory", "Directory & GPO"],
  ["resources", "Resources"],
] as const;
export default async function TechnologyPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ view?: string; position?: string }>;
}) {
  const { holdingSlug } = await params,
    query = await searchParams;
  const localContext = await requireWorkContext(holdingSlug);
  if (localContext.branchId || localContext.departmentId) {
    const { db } = await iamContext(holdingSlug);
    const data = await peopleLifecycleData(db, localContext);
    return (
      <div className="content-stack">
        <PageHeader
          eyebrow="Technology"
          title="Position readiness"
          description="Review preparation for this workspace. Apply resources from the Company workspace."
        />
        <Link
          href={`/holdings/${encodeURIComponent(holdingSlug)}/technology?view=positions`}
        >
          Open Company Technology console
        </Link>
        <PositionPreparation
          base={workspaceBase(publicScope(localContext))}
          data={{ ...data, canPrepare: false }}
        />
      </div>
    );
  }
  const resolved = await (async () => {
    try {
      const { db, actor } = await iamContext(holdingSlug),
        context = await requireWorkContext(holdingSlug);
      const [lifecycle, automation, technology, access, management] =
        await Promise.all([
          peopleLifecycleData(db, context),
          directoryAutomationData(db, actor),
          technologyData(db, actor),
          peopleAccessData(db, actor),
          readManagement(db, actor),
        ]);
      return {
        actor,
        context,
        lifecycle,
        automation,
        technology,
        access,
        management,
      };
    } catch (e) {
      return pageError(e, holdingSlug);
    }
  })();
  const { actor, lifecycle, automation, technology, access, management } =
      resolved,
    scope = publicScope(resolved.context),
    base = workspaceBase(scope),
    view = views.some(([id]) => id === query.view) ? query.view : "positions",
    meta = accessMetadata(access);
  return (
    <div className="content-stack" data-source="live">
      <PageHeader
        eyebrow="Technology"
        title="Technology console"
        description="Prepare position packages, apply employee resources, and manage Directory & Group Policy."
      />
      <nav className={styles.tabs} aria-label="Technology views">
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
      {view === "positions" && (
        <PositionPreparation
          base={base}
          data={lifecycle}
          positionId={query.position}
        />
      )}
      {view === "employees" && (
        <ProvisioningWorkspace
          base={base}
          lifecycle={lifecycle}
          data={technology}
          canApply={hasPermission(actor, "iam.directory.apply")}
        />
      )}
      {view === "directory" && (
        <DirectoryWorkspace base={base} initial={automation} />
      )}
      {view === "resources" && (
        <div className={styles.stack}>
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
          <details className="card">
            <summary>Accounts, ERPX groups & access policies</summary>
            <UserManagement
              initial={management}
              initialView="groups"
              views={["people", "groups", "policies"]}
              roles={meta.roles}
              base={base}
              api={workspaceApiHref(scope, "people/access")}
              accounts={{
                meta,
                initialEntries: accessEntries(access, new URLSearchParams()),
              }}
            />
          </details>
          <PreparationTasks base={base} data={lifecycle} />
          <p className="muted">
            Drive and Printer settings are managed through GPO. File shares and
            filesystem ACLs are managed in the file server.
          </p>
        </div>
      )}
    </div>
  );
}
