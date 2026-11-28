import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { RecordCatalog } from "@erpx/records-ui/components/record-catalog";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { talentWorkspace } from "@erpx/talent-server/public/talent";
import { nativeTalentWorkspace } from "@erpx/talent-ui/public/native-talent-page";
import { TalentConsole } from "@erpx/talent-ui/components/talent-console";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import { PageContent, Card } from "@erpx/platform-ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await (async () => {
      const workspace = await talentWorkspace(a),
        native = await nativeTalentWorkspace(a);
      return native
        ? {
            ...workspace,
            requisitions: native.requisitions as unknown as typeof workspace.requisitions,
          }
        : workspace;
    })().catch((e) => pageError(e, holdingSlug));
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
