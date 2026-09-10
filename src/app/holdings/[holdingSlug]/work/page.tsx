import { countOptions } from "@/server/inventory-counts";
import { transferOptions } from "@/server/inventory-transfers";
import { CountEditor } from "@/components/modules/stock-count-controls";
import { TransferEditor } from "@/components/modules/stock-transfer-controls";
import Link from "next/link";
import { notFound } from "next/navigation";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { staffPage } from "@/server/staff-page";
import { expenseOptions } from "@/server/expenses";
import { workspaceBase } from "@/lib/workspace";
import { queues } from "@/lib/work-queue";
import { requestPageOptions } from "@/server/request-page";
import { getEmployeeRequest } from "@/server/employee-requests";
import { RequestEditor } from "@/components/workplace/request-controls";
import { workTypes, workResponsibilities } from "@/lib/work-item";
import { PageHeader } from "@/components/ui/primitives";
import { TaskEditor } from "@/components/modules/staff-work-controls";
import { LeaveControls } from "@/components/modules/leave-controls";
import { ExpenseEditor } from "@/components/modules/expense-controls";
import { getWorkCapabilities } from "@/server/work-capabilities";
import { getWorkSummary } from "@/server/work-summary";
import { WorkDashboard } from "@/components/workplace/work-dashboard";

export default async function WorkPage({
  params,
  searchParams,
}: PageProps<"/holdings/[holdingSlug]/work">) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    query = await searchParams;
  const queue = queues.some(([id]) => id === query.queue)
    ? String(query.queue)
    : "todo";
  const type = workTypes.some(([id]) => id === query.type)
    ? String(query.type)
    : "all";
  const responsibility = workResponsibilities.some(
    ([id]) => id === query.responsibility,
  )
    ? String(query.responsibility)
    : "all";
  const q = typeof query.q === "string" ? query.q : "",
    newKind = typeof query.new === "string" ? query.new : "";
  const current = new URLSearchParams({
    queue,
    ...(responsibility !== "all" ? { responsibility } : {}),
    ...(type !== "all" ? { type } : {}),
    ...(q ? { q } : {}),
  }).toString();
  if (!newKind) {
    const summary = await getWorkSummary(actor);
    return (
      <main className="content" data-source="live">
        <WorkDashboard
          key={`${actor.companyId}:${actor.userId}:${base}:${current}:${summary.generatedAt}`}
          base={base}
          beta={!!actor.beta}
          initial={summary}
          queue={queue}
          type={type}
          responsibility={responsibility}
          q={q}
          current={current}
        />
      </main>
    );
  }
  const { write } = await holdingResources(actor.holdingId),
    capabilities = await getWorkCapabilities(write, actor);
  if (
    !capabilities.creation.some(
      (c) => c.kind === newKind && c.kind !== "conversation",
    )
  )
    notFound();
  const create = newKind === "task" ? await staffPage(actor.holdingSlug) : null;
  const requestKind =
    newKind === "service_request"
      ? "service"
      : newKind === "equipment_request"
        ? "equipment"
        : newKind === "purchase_request"
          ? "purchase"
          : null;
  const parentId =
    requestKind && typeof query.from === "string" ? query.from : null;
  const parent = parentId
    ? await getEmployeeRequest(write, actor, parentId)
    : null;
  const requestOptions = requestKind
    ? await requestPageOptions(write, actor)
    : null;
  if (parent && requestOptions) {
    requestOptions.branchId = parent.branch_id;
    requestOptions.departmentId = parent.department_id;
  }
  return (
    <main className="content" data-source="live">
      <PageHeader
        title="Your work"
        description="Your next steps, across the workspace."
      />
      <section className="work-create">
        <Link href={`?${current}`} className="back-link">
          Close new work
        </Link>
        {create && (
          <TaskEditor
            base={base}
            options={create.options}
            projects={create.projects}
            userId={actor.userId}
          />
        )}
        {newKind === "leave" && <LeaveControls base={base} initialOpen />}
        {requestKind && requestOptions && (
          <RequestEditor
            base={base}
            kind={requestKind}
            options={requestOptions}
            parentRequestId={parentId}
            returnTo={current}
          />
        )}
        {newKind === "expense" && (
          <ExpenseEditor
            base={base}
            options={await expenseOptions(actor)}
            initialOpen
          />
        )}
        {newKind === "stock_count" && (
          <CountEditor
            base={base}
            options={await countOptions(actor)}
            initialOpen
          />
        )}
        {newKind === "transfer" && (
          <TransferEditor
            base={base}
            options={await transferOptions(actor)}
            initialOpen
          />
        )}
      </section>
    </main>
  );
}
