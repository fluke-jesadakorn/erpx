import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { talentDetail } from "@/server/modules/talent/public/talent";
import { TalentConsole } from "@/components/records/loaders/modules--talent-console";
import { RecordDocumentPanel } from "@/components/records/loaders/attachments--record-document-panel";
import { hasPermission } from "@/lib/identity_access/permissions";
import { HireHandoffEditor } from "@/components/talent/hire-handoff-editor";
import { hireHandoffWorkspace } from "@/server/modules/people_payroll/public/hire-handoff-workspace";
import { PageContent, Card } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await talentDetail(a, id).catch((e) => pageError(e, holdingSlug)),
    x = data.application,
    canReadDocuments = hasPermission(a, "talent.documents.read"),
    hire = data.handoff&&hasPermission(a,"people.manage")&&!a.branchId&&!a.departmentId?await hireHandoffWorkspace(a,String(data.handoff.id)).catch(e=>pageError(e,holdingSlug)):null;
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/talent`)}>Talent</Link>
      <h1>{String(x.candidate_name)}</h1>
      <p>
        {String(x.job_title)} · {String(x.status)} · private until{" "}
        {String(x.retention_until)}
      </p>
      <TalentConsole base={base} detail={data} />
      {!a.branchId&&!a.departmentId&&["people.manage","legal.hold","legal.hold.release","legal.destroy"].some(permission=>hasPermission(a,permission))&&<Link href={canonicalUiHref(`${base}/talent/candidates/${String(x.candidate_id)}/privacy`)}>Review candidate privacy</Link>}
      {canReadDocuments && (
        <RecordDocumentPanel appearance="standard"
          base={base}
          targetType="talent_candidate"
          targetId={String(x.candidate_id)}
          expectedTargetVersion={Number(x.candidate_version)}
          editable={data.canManageDocuments&&(data.purchases===undefined||data.purchases["recruit.candidates"]?.settleExisting===true)}
          title="Candidate documents"
          purposes={[
            {
              key: "cv",
              label: "CV / résumé",
              description: "Attach the candidate-provided CV for this retained candidate record.",
            },
            {
              key: "portfolio",
              label: "Portfolio",
              description: "Attach candidate portfolio material relevant to this application.",
            },
            {
              key: "work_sample",
              label: "Work sample",
              description: "Attach a candidate-provided work sample with recorded consent.",
            },
            {
              key: "background_document",
              label: "Background document",
              description: "Attach authorized background or eligibility documents.",
              visibility:
                "Restricted to the assigned Talent owner and dedicated Talent document managers.",
            },
          ]}
        />
      )}
      {canReadDocuments && data.offer && (
        <RecordDocumentPanel appearance="standard"
          base={base}
          targetType="talent_offer"
          targetId={String(data.offer.id)}
          expectedTargetVersion={Number(data.offer.version)}
          editable={data.canManageDocuments&&data.offerDocumentsEditable&&(data.purchases===undefined||data.purchases["recruit.offers"]?.settleExisting===true)}
          title="Offer documents"
          purposes={[
            {
              key: "offer_document",
              label: "Offer document",
              description:
                "Attach the reviewed or signed offer revision. Candidate acceptance text remains separate workflow evidence.",
            },
          ]}
        />
      )}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Application history</h2>
        {data.history.map((h) => (
          <p key={String(h.id)}>
            Version {String(h.version)} · {String(h.action)} ·{" "}
            {String(h.reason)}
          </p>
        ))}
      </Card>
      {data.handoff && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Onboarding handoff</h2>
          <p>Accepted offer version {String(data.handoff.source_offer_version)}</p>
          {hire?<HireHandoffEditor base={base} data={hire} owner={{userId:a.userId,companyId:a.companyId}}/>:<Link href={canonicalUiHref(`${base}/people-organization/people/hire-handoffs/${String(data.handoff.id)}`)}>Open Company HR handoff</Link>}
        </Card>
      )}
    </PageContent>
  );
}
