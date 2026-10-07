import { RecordCatalog } from "@/components/records/record-catalog";
import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { talentWorkspace } from "@/server/modules/talent/public/talent";
import { TalentConsole } from "@/components/modules/talent-console";
import {hasPermission} from "@/lib/permissions";
import { PageContent, Card } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await talentWorkspace(a).catch((e) => pageError(e, holdingSlug));
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={`${base}/work`}>Work</Link>
      <h1>Talent</h1>
      <p>
        Control private candidate data from approved requisition through
        reviewed offer and onboarding handoff.
      </p>
      <TalentConsole base={base} data={data} />
      {hasPermission(a,"people.manage")&&!a.branchId&&!a.departmentId&&<Link href={`${base}/people/hire-handoffs`}>Consume accepted offers into HR onboarding →</Link>}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Applications</h2>
        {data.applications.map((x) => (
          <Link
            className="ui-record-item"
            key={String(x.id)}
            href={`${base}/talent/${x.id}`}
          >
            {String(x.candidate_name)} · {String(x.job_title)} ·{" "}
            {String(x.status)}
          </Link>
        ))}
      </Card>
    <RecordCatalog base={base} kind="talent_requisition" title="Requisitions" rows={data.requisitions} label="job_title"/><RecordCatalog base={base} kind="talent_candidate" title="Candidates" rows={data.candidates} label="name"/></PageContent>
  );
}
