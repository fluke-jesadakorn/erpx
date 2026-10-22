import { forbidden, redirect } from "next/navigation";
import { iamPresentationQueries } from "@erpx/server-identity_access/public/presentation-context-queries";
import { StandardOrganizationAccessWorkspace } from "@erpx/feature-identity_access/components/technology/standard-organization-access-workspace";
import { TechnologySectionWorkspace } from "@erpx/feature-identity_access/components/technology/section-workspace";
import { PageHeader } from "@erpx/ui/components/primitives";
import { AccessPackages, PreparationTasks } from "@erpx/feature-people_payroll/components/lifecycle-workspace";
import { UserManagementWorkspace } from "@erpx/feature-identity_access/components/user-management-workspace";
import { StandardDirectoryWorkspace } from "@erpx/feature-identity_access/components/technology/standard-directory-workspace";
import { IamConsole } from "@erpx/feature-iam/components/iam-console";
import { AccountInvitations } from "@erpx/feature-identity_access/components/account-invitations";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { accessMetadata, accessEntries } from "@erpx/server-people_payroll/public/people-access";
import { workspaceBase, workspaceApiHref } from "@erpx/feature-work_projection/lib/workspace";
import { technologyHref, technologySections } from "@erpx/feature-identity_access/lib/technology-navigation";
import type { RouteSearch } from "@erpx/feature-shared/lib/route-context";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { pageError } from "@erpx/server-work_projection/public/page-error";

type TechnologyPageProps = { params: Promise<{ holdingSlug: string }>; searchParams: Promise<RouteSearch> };
export default async function TechnologyPage(props: TechnologyPageProps) {
  const {holdingSlug} = await props.params;
  return TechnologyContent(props).catch(error => pageError(error, holdingSlug));
}
async function TechnologyContent({params, searchParams}: TechnologyPageProps) {
  const { holdingSlug } = await params, query = await searchParams;
  const local = await requireWorkContext(holdingSlug), scope = publicScope(local), base = workspaceBase(scope);
  if (query.view === "directory" && !query.section && !query.object && ["position","person","employee","department","branch","level"].some(key => query[key])) redirect(technologyHref(base, query, "organization"));
  if (query.view === "attributes") redirect(technologyHref(base, query, "settings", "attributes"));
  if (query.object && query.view !== "directory") redirect(technologyHref(base, query, "directory", "objects"));
  const view = query.view === "resources" || query.view === "directory" || query.view === "settings" ? query.view : "organization";
  const { queries, actor } = await iamPresentationQueries(holdingSlug);
  const canReadDirectory = hasPermission(actor, "iam.read") && !local.branchId && !local.departmentId;
  if (view === "directory" && !canReadDirectory || view !== "organization" && !hasPermission(actor,"iam.read")) forbidden();
    if (view === "organization") {
      const [organization, lifecycle, automation, technology, gpoRules] = await Promise.all([
        queries.organizationAccessData(local), queries.peopleLifecycleData(local),
        canReadDirectory ? queries.directoryAutomationData(actor) : Promise.resolve({ cases: [], connections: [], jobs: [], canApply: false, canManage: false, canReview: false }),
        canReadDirectory ? queries.technologyData(actor) : Promise.resolve({ applications: [], operations: [] }),
        queries.gpoRulesOverview(local),
      ]);
      return <div className="content-stack" data-source="live"><PageHeader appearance="standard" eyebrow="Technology" title="Organization & Access" description="Choose a unit to review positions, people, policies and delivery."/><StandardOrganizationAccessWorkspace base={base} companyName={organization.nodes.find(node => node.kind === "company")?.display_name ?? local.holdingName} organization={organization} lifecycle={lifecycle} automation={automation} technology={technology} gpoRules={gpoRules} canApply={canReadDirectory && hasPermission(actor,"iam.directory.apply")} initialSection={typeof query.section === "string" ? query.section : undefined}/></div>;
    }
    const section = technologySections[view].find(item => item.id === query.section)?.id ?? technologySections[view][0].id;
    const title = view === "resources" ? "Resources" : view === "directory" ? "Directory" : "Access settings";
    let content: React.ReactNode;
    if (view === "directory") {
      if (section === "connections") {
        const data = await queries.iamOverview(actor, false);
        content = <IamConsole embedded presentation="workspace" data={data} base={base} api={`/api/v1${base}/iam`} section="connections"/>;
      } else {
        const [automation, management] = await Promise.all([queries.directoryAutomationData(actor), section === "mappings" ? queries.readManagement(actor) : Promise.resolve(null)]);
        content = <StandardDirectoryWorkspace appearance="standard" embedded section={section as "objects"|"mappings"|"work"} base={base} initial={automation} mode={section === "mappings" ? "settings" : "inventory"} attributes={management?.attributes} people={management?.people} initialConnectionId={typeof query.connection === "string" ? query.connection : undefined} initialObjectId={typeof query.object === "string" ? query.object : undefined}/>;
      }
    } else if (view === "settings" && section !== "attributes") {
      const data = await queries.iamOverview(actor, false);
      content = <><IamConsole embedded presentation="workspace" data={data} base={base} api={`/api/v1${base}/iam`} section={section as "roles"|"matrix"|"groups"|"reviews"}/>{section === "reviews" && hasPermission(local,"iam.roles.manage") && <AccountInvitations actor={local} base={base}/>}</>;
    } else {
      const [access, management, lifecycle] = await Promise.all([
        queries.peopleAccessData(actor), queries.readManagement(actor), view === "resources" && (section === "packages" || section === "tasks") ? queries.peopleLifecycleData(local) : Promise.resolve(null),
      ]);
      const meta = accessMetadata(access);
      if (view === "resources" && section === "packages" && lifecycle) content = <AccessPackages base={base} data={lifecycle} roles={access.overview.roles} groups={management.groups} connections={access.overview.connections.filter(connection => connection.status === "active").map(connection => ({id:String(connection.id),name:String(connection.name),provider:String(connection.provider)}))} directoryGroups={access.overview.groups.map(group => ({id:String(group.external_id),name:String(group.name),connectionId:String(group.connection_id)}))}/>;
      else if (view === "resources" && section === "tasks" && lifecycle) content = <PreparationTasks base={base} data={lifecycle}/>;
      else content = <UserManagementWorkspace compactConsole initial={management} initialView={section === "accounts" ? "people" : section === "groups" ? "groups" : section === "policies" ? "policies" : "attributes"} views={[section === "accounts" ? "people" : section === "groups" ? "groups" : section === "policies" ? "policies" : "attributes"]} roles={meta.roles} base={base} api={workspaceApiHref(scope,"people/access")} accounts={{meta,initialEntries:accessEntries(access,new URLSearchParams())}}/>;
    }
    return <div className="content-stack" data-source="live"><PageHeader appearance="standard" eyebrow="Technology" title={title} description="Choose a section, inspect records and review changes."/><TechnologySectionWorkspace base={base} view={view} section={section} title={title}>{content}</TechnologySectionWorkspace></div>;
}
