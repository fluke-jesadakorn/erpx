import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { notFound, redirect } from "next/navigation";
import { PeopleHrSettings } from "@erpx/feature-people_payroll/components/people-hr-settings";
import { PageHeader } from "@erpx/ui/components/primitives";
import { peopleHrTopics,peopleHrSettingsData,type PeopleHrTopic } from "@erpx/server-people_payroll/public/people-hr-settings";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import type { PeopleRouteSearch } from "@erpx/feature-people_payroll/lib/people-route-context";
import { technologySettingsDestination } from "@erpx/feature-identity_access/lib/technology-navigation";

export default async function PeopleSettings({ params, searchParams }: {
  params: Promise<{ holdingSlug: string }>; searchParams: Promise<PeopleRouteSearch>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const base = workspaceBase(publicScope(actor)), query = await searchParams;
  const section = typeof query.section === "string" ? query.section : "";
  if (["connections", "roles", "matrix", "groups", "reviews", "attributes"].includes(section)) redirect(technologySettingsDestination(base, query, section));
  if (section === "my-access") redirect("/my-access");
  if (["users", "overview"].includes(section)) redirect(canonicalUiHref(`${base}/people?view=users`));
  if (section && !peopleHrTopics.includes(section as typeof peopleHrTopics[number])) notFound();
  const action = query.new === "leave-policy" ? "policy" : typeof query.editor === "string" ? query.editor : undefined;
  const initialIndex=await peopleHrSettingsData(actor) as {topics:PeopleHrTopic[]};
  const initialData=section?await peopleHrSettingsData(actor,section as PeopleHrTopic):undefined;
  return <div className="content-stack" data-source="live">
    <PageHeader appearance="standard" eyebrow="People" title="HR settings" description="Manage Company policies and defaults." />
    <PeopleHrSettings standalone navigation={false} appearance="standard" base={base} initialTopic={section} initialAction={action} initialIndex={initialIndex} initialData={initialData} />
  </div>;
}
