import { FeaturePage, FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { IamConsole } from "@/components/iam/iam-console";
import { OrganizationWorkspace } from "@/components/modules/organization-workspace";
import { PeopleNavigation } from "@/components/modules/people-navigation";
import { ServiceNavigation } from "@/components/modules/service-navigation";
import { DirectoryWorkspace } from "@/components/people/directory-workspace";
import { HoldingTemplates } from "@/components/people/holding-templates";
import { LifecycleSettings } from "@/components/people/lifecycle-workspace";
import { UserManagement } from "@/components/people/user-management";
import { SelfServiceSettings } from "@/components/self-service/settings";
import { PageHeader } from "@/components/ui/primitives";
import { iamSections,type IamSection } from "@/lib/iam";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { administrationAccess } from "@/server/administration";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { directoryAutomationData } from "@/server/directory-operations";
import { ApiError } from "@/server/http";
import { iamContext } from "@/server/iam-context";
import { iamOverview } from "@/server/iam-service";
import { leaveAdministration } from "@/server/leave-self-service";
import { organizationOverview } from "@/server/organization";
import {
accessEntries,
accessMetadata,
peopleAccessData,
} from "@/server/people-access";
import { peopleLifecycleData } from "@/server/people-lifecycle";
import { peopleTemplatesData } from "@/server/people-templates";
import { holdingResources } from "@/server/runtime";
import { readManagement } from "@/server/user-management";
import { notFound } from "next/navigation";

export default async function PeopleSettings({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ section?: string }>;
}) {
  const slug = (await params).holdingSlug,
    actor = await requireWorkContext(slug),
    base = workspaceBase(publicScope(actor)),
    section =
      (await searchParams).section ??
      (hasPermission(actor, "organization.manage")
        ? "templates"
        : actor.beta
          ? "connections"
          : "attributes");
  
  if (
    !["leave", "templates", "attributes", ...iamSections.map(([id]) => id)].includes(
      section,
    )
  )
    notFound();
  const technology =
    !actor.beta &&
    ["connections", "roles", "matrix", "reviews", "groups"].includes(section);
  const feature = section === "matrix" ? "roles" : section;
  const featureTitle =
    feature === "groups"
      ? "Groups & policies"
      : (iamSections.find(([id]) => id === feature)?.[1] ?? "People settings");
  const admin = technology ? await administrationAccess(actor) : null;
  const { write } = await holdingResources(actor.holdingId);
  const iam =
    hasPermission(actor, "iam.read") || section === "my-access"
      ? await iamContext(slug)
      : null;
  const data = iam
    ? await iamOverview(iam.db, iam.actor, section === "my-access")
    : null;
  const management =
    iam && section === "connections"
      ? await readManagement(iam.db, iam.actor)
      : null;
  return (
    <FeaturePage className="content-stack" data-source="live">
      <PageHeader
        eyebrow={technology ? "Technology" : "People"}
        title={technology ? featureTitle : "People settings"}
        description={
          technology
            ? "Manage directory connections, security, and access configuration."
            : "HR templates, lifecycle handoffs, and custom attributes."
        }
      />
      {technology ? (
        <ServiceNavigation
          context={{
            workspaceBase: base,
            scopeKind: actor.scope.kind,
            role: actor.role,
            permissions: actor.iam?.permissions,
            adminSections: admin?.sections,
          }}
          groupId="technology"
          activeId={`technology-${feature}`}
        />
      ) : (
        <PeopleNavigation
          base={base}
          actor={actor}
          scopeKind={actor.scope.kind}
          active="settings"
        />
      )}
      <WidgetGrid>
{!technology && <>{await (async()=>{try{return <SelfServiceSettings base={base} data={await leaveAdministration(actor)}/>;}catch(error){if(error instanceof ApiError&&error.status===403)return <p>HR setup or permission is required to manage leave and overtime.</p>;throw error;}})()}</>}
      {!technology && hasPermission(actor, "organization.manage") && <FeatureWidget id="templates" title="Templates & lifecycle handoff">
          <>
            <LifecycleSettings
              base={base}
              data={await peopleLifecycleData(write, actor)}
              owners={
                data?.users.filter((user) =>
                  data.assignments.some(
                    (a) =>
                      a.user_id === user.id &&
                      a.active &&
                      a.scope_kind === "holding" &&
                      data.roles.some(
                        (role) =>
                          role.id === a.role &&
                          role.permissions.includes("iam.directory.manage"),
                      ),
                  ),
                ) ?? []
              }
            />
            {iam?.actor.scopeKind === "holding" && (
              <HoldingTemplates
                base={base}
                data={await peopleTemplatesData(iam.db, iam.actor)}
              />
            )}
            <details className="card">
              <summary>
                Organization templates and advanced HR configuration
              </summary>
              <OrganizationWorkspace
                base={base}
                data={await organizationOverview(write, actor)}
                initialView="templates"
              />
            </details>
          </>
        </FeatureWidget>}
      {!technology && iam && <FeatureWidget id="attributes" title="Custom attributes">{
        (await (async () => {
          const access = await peopleAccessData(iam.db, iam.actor);
          return (
            <UserManagement
              initial={await readManagement(iam.db, iam.actor)}
              roles={accessMetadata(access).roles}
              base={base}
              api={`/api/v1${base}/people/access`}
              initialView="attributes"
              views={["attributes"]}
              accounts={{
                meta: accessMetadata(access),
                initialEntries: accessEntries(access, new URLSearchParams()),
              }}
            />
          );
        })())}</FeatureWidget>}
      {iam && data && iamSections.some(([id]) => id === section) && (feature === "roles" ? (["roles","matrix"] as IamSection[]) : [section as IamSection]).map(panel=><FeatureWidget key={panel} id={panel} title={iamSections.find(([id])=>id===panel)?.[1]??panel}><IamConsole
          embedded
          data={data}
          base={base}
          api={`/api/v1${base}/iam`}
          section={panel}
        /></FeatureWidget>)}
      {section === "connections" && iam?.actor.scopeKind === "holding" && (
        <DirectoryWorkspace
          base={base}
          initial={await directoryAutomationData(iam.db, iam.actor)}
          mode="settings"
          attributes={management?.attributes ?? []}
          people={management?.people ?? []}
        />
      )}
      {!iam && !hasPermission(actor, "organization.manage") && (
        <p>Your selected role does not manage these settings.</p>
      )}
    </WidgetGrid></FeaturePage>
  );
}
