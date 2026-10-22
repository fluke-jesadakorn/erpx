import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { RecordDocumentPanel } from "@erpx/feature-records/components/loaders/attachments--record-document-panel";
import { BenefitsConsole } from "@erpx/feature-records/components/loaders/modules--benefits-console";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { benefitDetail } from "@erpx/server-benefits/public/benefits";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { PageContent, Card } from "@erpx/ui/components/foundation";

export const dynamic = "force-dynamic";

export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const data = await benefitDetail(actor, id).catch((error) =>
    pageError(error, holdingSlug),
  );
  const election = data.election;
  const selfCanEditElection =
    data.isOwner && ["elected", "submitted"].includes(String(election.status));
  const providerVisible =
    data.canManageDocuments ||
    (data.isOwner && ["acknowledged", "released"].includes(String(election.status)));

  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/benefits`)}>Benefits</Link>
      <h1>{String(election.plan_name)}</h1>
      <p>
        {String(election.employee_name)} · {String(election.status)} · version{" "}
        {String(election.version)}
      </p>
      <BenefitsConsole base={base} detail={data} />
      <RecordDocumentPanel appearance="standard"
        base={base}
        targetType="benefit_plan"
        targetId={String(election.plan_id)}
        expectedTargetVersion={Number(election.plan_current_version)}
        editable={data.canManageDocuments && data.purchase?.newWork !== false}
        title="Plan booklet"
        purposes={[
          {
            key: "benefit_booklet",
            label: "Benefit booklet",
            description:
              "Review the controlled plan booklet or coverage guide for the current plan revision.",
            visibility:
              "Published plan documents are self-visible to active employees.",
          },
        ]}
      />
      <RecordDocumentPanel appearance="standard"
        base={base}
        targetType="benefit_election"
        targetId={String(election.id)}
        expectedTargetVersion={Number(election.version)}
        editable={(data.canManageDocuments || selfCanEditElection) && data.purchase?.settleExisting !== false}
        title="Election documents"
        purposes={[
          {
            key: "election_document",
            label: "Election document",
            description:
              "Review employee coverage-selection documents and later controlled revisions.",
            visibility:
              "Only the employee owner and dedicated Benefits document roles can access these files.",
          },
        ]}
      />
      {providerVisible && (
        <RecordDocumentPanel appearance="standard"
          base={base}
          targetType="benefit_election"
          targetId={String(election.id)}
          expectedTargetVersion={Number(election.version)}
          editable={data.canManageDocuments && data.purchase?.settleExisting !== false}
          title="Provider documents"
          purposes={[
            {
              key: "provider_proof",
              label: "Provider proof",
              description:
                "Review the provider acknowledgement, receipt, or coverage confirmation.",
              visibility:
                "Provider proof becomes self-visible only after acknowledgement is recorded.",
            },
          ]}
        />
      )}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>History</h2>
        {data.history.map((event) => (
          <p key={String(event.id)}>
            Version {String(event.version)} · {String(event.action)} ·{" "}
            {String(event.reason)}
          </p>
        ))}
      </Card>
      {data.payroll && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Payroll input</h2>
          <p>
            Released from election version{" "}
            {String(data.payroll.source_election_version)} ·{" "}
            {String(data.payroll.currency)}{" "}
            {String(data.payroll.employee_deduction_minor)} minor units employee
            deduction
          </p>
        </Card>
      )}
    </PageContent>
  );
}
