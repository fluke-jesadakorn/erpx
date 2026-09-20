import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { PageHeader } from "@/components/ui/primitives";
import { WorkDashboard } from "@/components/workplace/work-dashboard";
import { CountEditor,ExpenseEditor,LeaveControls,RequestEditor,TaskEditor,TransferEditor } from "@/components/workplace/work-editors";
import { workResponsibilities,workTypes,workViews } from "@/lib/work-item";
import { queues } from "@/lib/work-queue";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { getEmployeeRequest } from "@/server/employee-requests";
import { expenseOptions } from "@/server/expenses";
import { countOptions } from "@/server/inventory-counts";
import { transferOptions } from "@/server/inventory-transfers";
import { requestPageOptions } from "@/server/request-page";
import { holdingResources } from "@/server/runtime";
import { staffPage } from "@/server/staff-page";
import { getWorkCapabilities } from "@/server/work-capabilities";
import { getWorkPage } from "@/server/work-summary";
import Link from "next/link";
import { notFound,redirect } from "next/navigation";

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
  const view = workViews.some(([id]) => id === query.view) ? String(query.view)
    : query.queue || query.responsibility ? "legacy" : "actions";
  const current = new URLSearchParams({
    ...(Number(query.offset)>0?{offset:String(query.offset)}:{}),
    ...(view !== "legacy" ? { view } : { queue }),
    ...(responsibility !== "all" ? { responsibility } : {}),
    ...(type !== "all" ? { type } : {}),
    ...(q ? { q } : {}),
  }).toString();
  if (!newKind) {
    const summary = await getWorkPage(actor,{view,queue,type,q,responsibility,offset:Number(query.offset??0)});
    return (
      <FeaturePage className="content" data-source="live">
        <WorkDashboard
          key={`${actor.companyId}:${actor.userId}:${base}:${current}:${summary.generatedAt}`}
          base={base}
          beta={!!actor.beta}
          initial={summary}
          queue={queue}
          view={view}
          type={type}
          responsibility={responsibility}
          q={q}
          current={current}
        />
      </FeaturePage>
    );
  }
  if(newKind === "leave" || newKind === "overtime")redirect(`${base}/${newKind}/new`);
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
    <FeaturePage className="content" data-source="live">
      <PageHeader
        title="Your work"
        description="Your next steps, across the workspace."
      /><FeatureTabs/><WidgetGrid pageKey="work-create">
      <FeatureWidget id="work-draft" title="New work">
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
          <FeatureWidget id="work-expense-details" title="Expense details"><ExpenseEditor
            base={base}
            options={await expenseOptions(actor)}
            initialOpen
          /></FeatureWidget>
        )}
        {newKind === "stock_count" && (
          <FeatureWidget id="work-stock-count" title="Stock count"><CountEditor
            base={base}
            options={await countOptions(actor)}
            initialOpen
          /></FeatureWidget>
        )}
        {newKind === "transfer" && (
          <FeatureWidget id="work-warehouse-transfer" title="Warehouse transfer"><TransferEditor
            base={base}
            options={await transferOptions(actor)}
            initialOpen
          /></FeatureWidget>
        )}
      </FeatureWidget>
    </WidgetGrid></FeaturePage>
  );
}
