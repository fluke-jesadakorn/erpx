import { recordsPresentationQueries } from "@/server/modules/records/public/presentation-queries";
import { RecordLink as Link } from "@/components/records/record-link";
import { notFound } from "next/navigation";
import { FeaturePage } from "@/components/shared/features/feature-page";
import { PageHeader } from "@/components/ui/primitives";
import { RequestEditor } from "@/components/workplace/request-controls";
import { requestKinds, type RequestKind } from "@/lib/people_payroll/employee-services";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";




export const dynamic = "force-dynamic";

export default async function NewRequest({
  params,
  searchParams,
}: PageProps<"/holdings/[holdingSlug]/requests/new">) {
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
      <Link className="back-link" href={`${base}/work${typeof query.return === "string" ? `?${query.return}` : "?view=tracking&tracking=requests"}`}>
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
