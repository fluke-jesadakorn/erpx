import {workforcePresentationContext} from "@erpx/server-people_payroll/public/workforce-presentation";
import {workforcePurchaseControls} from "@erpx/server-people_payroll/public/workforce-purchases";
import {TransferEnvironment} from "@erpx/feature-inventory/components/transfer-purchase-context";
import {transferPurchaseControls} from "@erpx/server-inventory/public/transfer-purchases";
import {countPurchaseControls} from "@erpx/server-inventory/public/count-purchases";
import {CountEnvironment} from "@erpx/feature-inventory/components/count-purchase-context";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { workProjectionPresentationQueries } from "@erpx/server-work_projection/public/presentation-queries";

import { PageContent } from "@erpx/ui/components/patterns";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { PageHeader } from "@erpx/ui/components/primitives";
import { CountEditor } from "@erpx/feature-inventory/components/lazy-count-editor";
import { ExpenseEditor } from "@erpx/feature-people_payroll/components/lazy-expense-editor";
import { LeaveControls } from "@erpx/feature-people_payroll/components/lazy-leave-controls";
import { RequestEditor } from "@erpx/workplace/components/lazy-request-editor";
import { TransferEditor } from "@erpx/feature-inventory/components/lazy-transfer-editor";

import { expenseOptions } from "@erpx/server-people_payroll/public/expenses";
import { countOptions } from "@erpx/server-inventory/public/inventory-counts";
import { transferOptions } from "@erpx/server-inventory/public/inventory-transfers";



import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { notFound,redirect } from "next/navigation";

import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
export async function CreateWork({actor,base,newKind,current,query}:{actor:WorkContext;base:string;newKind:string;current:string;query:Record<string,string|string[]|undefined>}) {
  if(newKind === "leave" || newKind === "overtime")redirect(canonicalUiHref(`${base}/${newKind}/new`));
  const presentation = await workProjectionPresentationQueries(actor.holdingId),
    capabilities = await presentation.getWorkCapabilities(actor);
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
    ? await presentation.getEmployeeRequest(actor, parentId)
    : null;
  const requestOptions = requestKind
    ? await presentation.requestPageOptions(actor)
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
        {newKind === "leave" && <LeaveControls base={base} initialOpen workContext={workforcePresentationContext(actor)} purchase={await workforcePurchaseControls(actor)}/>}
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
          <CountEnvironment purchase={await countPurchaseControls(actor)}><FeatureWidget appearance="standard" id="work-stock-count" title="Stock count"><CountEditor
            base={base}
            options={await countOptions(actor)}
            initialOpen
          /></FeatureWidget></CountEnvironment>
        )}
        {newKind === "transfer" && (
          <TransferEnvironment purchase={await transferPurchaseControls(actor)}><FeatureWidget appearance="standard" id="work-warehouse-transfer" title="Warehouse transfer"><TransferEditor
            base={base}
            options={await transferOptions(actor)}
            initialOpen
          /></FeatureWidget></TransferEnvironment>
        )}
      </FeatureWidget>
    </WidgetGrid></PageContent>
  );
}
