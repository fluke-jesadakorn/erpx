import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { recordsPresentationQueries } from "@erpx/server-records/public/presentation-queries";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { notFound } from "next/navigation";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { PageHeader } from "@erpx/ui/components/primitives";
import { RequestEditor } from "@erpx/workplace/components/request-controls";
import { requestKinds, type RequestKind } from "@erpx/feature-people_payroll/lib/employee-services";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";




export const dynamic = "force-dynamic";

export default async function NewRequest({
  params,
  searchParams,
}: { params: Promise<{ holdingSlug: string;  }>; searchParams: Promise<Record<string,string|string[]|undefined>> }) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const query = await searchParams;
  const kind = typeof query.kind === "string" ? query.kind : "purchase";
  if (!requestKinds.some(([id]) => id === kind)) notFound();
  const parentRequestId = typeof query.from === "string" ? query.from : null;
  const presentation = await recordsPresentationQueries(actor.holdingId);
  const options = await presentation.requestPageOptions(actor);
  if (parentRequestId) {
    const parent = await presentation.getEmployeeRequest(actor, parentRequestId);
    if (parent.kind !== "service" || kind === "service") notFound();
    options.branchId = parent.branch_id;
    options.departmentId = parent.department_id;
  }
  const label = requestKinds.find(([id]) => id === kind)?.[1] ?? "Request";

  return (
    <FeaturePage className="ui-page-content request-page" data-source="live">
      <Link className="back-link" href={canonicalUiHref(`${base}/work${typeof query.return === "string" ? `?${query.return}` : "?view=tracking&tracking=requests"}`)}>
        ← My work
      </Link>
      <PageHeader appearance="standard"
        eyebrow="Personal requests"
        title={label}
        description={kind === "purchase"
          ? "Describe what you need and where it is needed. Your purchasing team takes over sourcing after approval."
          : "Describe what you need. Your request will reach the right team."}
      />
      <RequestEditor
        base={base}
        kind={kind as RequestKind}
        options={options}
        parentRequestId={parentRequestId}
        returnTo={typeof query.return === "string" ? query.return : ""}
      />
    </FeaturePage>
  );
}
