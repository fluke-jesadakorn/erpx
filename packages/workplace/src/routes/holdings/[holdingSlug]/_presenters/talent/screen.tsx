import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordCatalog } from "@erpx/feature-records/components/record-catalog";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { talentWorkspace } from "@erpx/server-talent/public/talent";
import { TalentConsole } from "@erpx/feature-talent/components/talent-console";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import { PageContent, Card } from "@erpx/ui/components/foundation";
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
      <Link href={canonicalUiHref(`${base}/work`)}>Work</Link>
      <h1>Talent</h1>
      <p>
        Control private candidate data from approved requisition through
        reviewed offer and onboarding handoff.
      </p>
      <TalentConsole base={base} data={data} />
      {hasPermission(a,"people.manage")&&!a.branchId&&!a.departmentId&&<Link href={canonicalUiHref(`${base}/people/hire-handoffs`)}>Consume accepted offers into HR onboarding →</Link>}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Applications</h2>
        {data.applications.map((x) => (
          <Link
            className="ui-record-item"
            key={String(x.id)}
            href={canonicalUiHref(`${base}/talent/${x.id}`)}
          >
            {String(x.candidate_name)} · {String(x.job_title)} ·{" "}
            {String(x.status)}
          </Link>
        ))}
      </Card>
    <RecordCatalog base={base} kind="talent_requisition" title="Requisitions" rows={data.requisitions} label="job_title"/><RecordCatalog base={base} kind="talent_candidate" title="Candidates" rows={data.candidates} label="name"/></PageContent>
  );
}
