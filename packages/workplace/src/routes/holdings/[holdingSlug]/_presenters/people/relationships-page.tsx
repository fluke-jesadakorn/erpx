import { PeopleAccessWorkspace } from "@erpx/feature-identity_access/components/people-access-workspace";
import { iamPresentationQueries } from "@erpx/server-identity_access/public/presentation-context-queries";
import { PeopleNavigation } from "@erpx/feature-people_payroll/components/people-navigation";
import { PageHeader } from "@erpx/ui/components/primitives";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase, workspaceApiHref } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
export async function RelationshipsPage({ params, classification = "external", section = "users" }: {
 params: Promise<{holdingSlug:string}>; classification?: "external" | "unclassified"; section?: "users" | "organizations" | "teams";
}) {
 const {holdingSlug}=await params, local=await requireWorkContext(holdingSlug);
 const {queries,actor}=await iamPresentationQueries(holdingSlug), base=workspaceBase(publicScope(local));
 const data=await queries.peopleRelationshipsSnapshot(actor).catch(error=>pageError(error,holdingSlug));
 return <div className="content-stack" data-source="live"><PageHeader appearance="standard" eyebrow="People" title={classification === "unclassified" ? "Unclassified" : "External collaborators"} description={classification === "unclassified" ? "Company accounts awaiting an employee or external relationship." : "Manage collaborators, organizations, teams and internal sponsors."}/><PeopleNavigation base={base} actor={local} scopeKind={local.scope.kind} active={classification}/><PeopleAccessWorkspace initial={data} api={workspaceApiHref(publicScope(local),"people/relationships")} base={base} classification={classification} initialView={section}/></div>;
}
