import { RecordCatalog } from "@/components/records/record-catalog";

import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { benefitsWorkspace } from "@/server/benefits";
import { BenefitsConsole } from "@/components/modules/benefits-console";
import { PageContent, Card } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";

export default async function Page({ params, searchParams }: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ view?: string }>;
}) {
  const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(actor)), personal = (await searchParams).view === "mine",
    data = await benefitsWorkspace(actor, personal).catch(error => pageError(error, holdingSlug));
  return <PageContent as="div" className="ui-page-content">
    <Link href={`${base}/work?view=for-me`}>My work</Link>
    <h1>{personal ? "My benefits" : "Benefits"}</h1>
    <p>{personal ? "Follow your benefit elections, review progress and provider acknowledgement." : "Review plans and dependents, enroll during controlled windows, record provider acknowledgement and release immutable payroll inputs."}</p>
    {personal ? <UiActionLink appearance="standard" variant="secondary" href={`${base}/benefits`}>Benefit plans & enrollment</UiActionLink> : <BenefitsConsole base={base} data={data} />}
    <Card as="section" appearance="standard" className="ui-panel"><h2>Elections</h2>
      {!data.elections.length && <p>No benefit elections are available in this view.</p>}
      {data.elections.map(election => <Link className="ui-record-item" key={String(election.id)} href={`${base}/benefits/${election.id}`}>{String(election.employee_name)} · {String(election.plan_name)} · {String(election.status)}</Link>)}
    </Card>
  <RecordCatalog base={base} kind="benefit_plan" title="Benefit plans" rows={data.plans} label="name"/></PageContent>;
}
