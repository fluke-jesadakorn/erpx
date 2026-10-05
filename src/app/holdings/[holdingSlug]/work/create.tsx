
import { PageContent } from "@/components/ui/patterns";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { PageHeader } from "@/components/ui/primitives";
import { CountEditor,ExpenseEditor,LeaveControls,RequestEditor,TransferEditor } from "@/components/workplace/work-editors";
import { getEmployeeRequest } from "@/server/employee-requests";
import { expenseOptions } from "@/server/expenses";
import { countOptions } from "@/server/inventory-counts";
import { transferOptions } from "@/server/inventory-transfers";
import { requestPageOptions } from "@/server/request-page";
import { holdingResources } from "@/server/runtime";
import { getWorkCapabilities } from "@/server/work-capabilities";
import { RecordLink as Link } from "@/components/records/record-link";
import { notFound,redirect } from "next/navigation";

import type { WorkContext } from "@/server/authorization";
export async function CreateWork({actor,base,newKind,current,query}:{actor:WorkContext;base:string;newKind:string;current:string;query:Record<string,string|string[]|undefined>}) {
  if(newKind === "leave" || newKind === "overtime")redirect(`${base}/${newKind}/new`);
  const { write } = await holdingResources(actor.holdingId),
    capabilities = await getWorkCapabilities(write, actor);
  if (
    !capabilities.creation.some(
      (c) => c.kind === newKind && c.kind !== "conversation",
    )
  )
    notFound();
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
    <PageContent as={FeaturePage} data-source="live">
      <PageHeader appearance="standard"
        title="Your work"
        description="Your next steps, across the workspace."
      /><FeatureTabs/><WidgetGrid pageKey="work-create">
      <FeatureWidget appearance="standard" id="work-draft" title="New work">
        <Link href={`?${current}`} className="back-link">
          Close new work
        </Link>
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
          <FeatureWidget appearance="standard" id="work-expense-details" title="Expense details"><ExpenseEditor
            base={base}
            options={await expenseOptions(actor)}
            initialOpen
          /></FeatureWidget>
        )}
        {newKind === "stock_count" && (
          <FeatureWidget appearance="standard" id="work-stock-count" title="Stock count"><CountEditor
            base={base}
            options={await countOptions(actor)}
            initialOpen
          /></FeatureWidget>
        )}
        {newKind === "transfer" && (
          <FeatureWidget appearance="standard" id="work-warehouse-transfer" title="Warehouse transfer"><TransferEditor
            base={base}
            options={await transferOptions(actor)}
            initialOpen
          /></FeatureWidget>
        )}
      </FeatureWidget>
    </WidgetGrid></PageContent>
  );
}
