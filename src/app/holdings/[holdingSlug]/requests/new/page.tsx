import Link from "next/link";
import { notFound } from "next/navigation";
import { FeaturePage } from "@/components/features/feature-page";
import { PageHeader } from "@/components/ui/primitives";
import { RequestEditor } from "@/components/workplace/request-controls";
import { requestKinds, type RequestKind } from "@/lib/employee-services";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { getEmployeeRequest } from "@/server/employee-requests";
import { requestPageOptions } from "@/server/request-page";
import { holdingResources } from "@/server/runtime";

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
  const { write } = await holdingResources(actor.holdingId);
  const options = await requestPageOptions(write, actor);
  if (parentRequestId) {
    const parent = await getEmployeeRequest(write, actor, parentRequestId);
    if (parent.kind !== "service" || kind === "service") notFound();
    options.branchId = parent.branch_id;
    options.departmentId = parent.department_id;
  }
  const label = requestKinds.find(([id]) => id === kind)?.[1] ?? "Request";

  return (
    <FeaturePage className="content-stack" data-source="live">
      <Link className="back-link" href={`${base}/home`}>
        ← Today
      </Link>
      <PageHeader
        eyebrow="Personal requests"
        title={kind === "purchase" ? "Buy something" : label}
        description={kind === "purchase"
          ? "Describe what you need and where it is needed. Your purchasing team takes over sourcing after approval."
          : "Describe what you need. Your request will reach the right team."}
      />
      <RequestEditor
        base={base}
        kind={kind as RequestKind}
        options={options}
        parentRequestId={parentRequestId}
      />
    </FeaturePage>
  );
}
