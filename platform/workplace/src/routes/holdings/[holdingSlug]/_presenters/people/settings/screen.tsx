import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { notFound, redirect } from "next/navigation";
import { PeopleHrSettings } from "@erpx/people-payroll-ui/components/people-hr-settings";
import { PageHeader } from "@erpx/platform-ui/components/primitives";
import { peopleHrTopics,peopleHrSettingsData,type PeopleHrTopic } from "@erpx/people-payroll-server/public/people-hr-settings";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import type { PeopleRouteSearch } from "@erpx/people-payroll-ui/lib/people-route-context";
import { technologySettingsDestination } from "@erpx/identity-access-ui/lib/technology-navigation";

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
