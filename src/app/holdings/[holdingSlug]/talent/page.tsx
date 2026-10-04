import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { talentWorkspace } from "@/server/talent";
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
    </PageContent>
  );
}
