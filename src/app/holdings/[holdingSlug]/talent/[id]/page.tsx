import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { talentDetail } from "@/server/modules/talent/public/talent";
import { TalentConsole } from "@/components/modules/talent-console";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import { hasPermission } from "@/lib/permissions";
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
    canReadDocuments = hasPermission(a, "talent.documents.read");
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={`${base}/talent`}>Talent</Link>
      <h1>{String(x.candidate_name)}</h1>
      <p>
        {String(x.job_title)} · {String(x.status)} · private until{" "}
        {String(x.retention_until)}
      </p>
      <TalentConsole base={base} detail={data} />
      {canReadDocuments && (
        <RecordDocumentPanel appearance="standard"
          base={base}
          targetType="talent_candidate"
          targetId={String(x.candidate_id)}
          expectedTargetVersion={Number(x.candidate_version)}
          editable={data.canManageDocuments}
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
          editable={data.canManageDocuments}
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
          <p>
            Open · accepted offer version{" "}
            {String(data.handoff.source_offer_version)}
          </p>
        </Card>
      )}
    </PageContent>
  );
}
