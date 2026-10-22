import {financeProductPresentationOptions} from "@erpx/server-finance/public/product-presentation-request";
import {financeActionAllowed} from "@erpx/server-finance/public/rules/purchase-controls";
import {financeDocumentPresentationAccess} from "@erpx/server-finance/public/record-presentation";
import {financePurchaseControls} from "@erpx/server-finance/public/purchases";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceRecordWorkspace } from "@erpx/feature-records/components/loaders/finance--finance-record-workspace";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {supplierBatchWorkspace} from "@erpx/server-procurement/public/supplier-payments";
import { SupplierBatchConsole } from "@erpx/feature-records/components/loaders/modules--supplier-batch-console";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import { RecordDocumentPanel } from "@erpx/feature-records/components/loaders/attachments--record-document-panel";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
export const dynamic='force-dynamic';
export default async function SupplierBatchPage({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 const data=await supplierBatchWorkspace(actor,id).catch(error=>pageError(error,holdingSlug)),purchases=await financePurchaseControls(actor),access=await financeDocumentPresentationAccess(actor,"supplier-batch",id,await financeProductPresentationOptions());
 const editable=hasPermission(actor,"finance.write")&&(financeActionAllowed(purchases,"settle-existing","finance.banking")||data.sourcePurchase?.settleExisting===true);
 return <FeaturePage className="ui-page-content" data-source="live"><FinanceRecordWorkspace sourceHistory={access.sourceOnly} purchases={purchases} base={base} active="cash" title="Supplier payment batch" description={`${data.batch?.planned_date} · ${data.batch?.currency} · ${data.batch?.status}`}><SupplierBatchConsole owner={{companyId:actor.companyId,userId:actor.userId,assignmentId:actor.assignmentId,branchId:actor.branchId,departmentId:actor.departmentId}} base={base} data={data}/>{data.batch&&<RecordDocumentPanel appearance="standard" base={base} targetType="finance_supplier_batch" targetId={id} expectedTargetVersion={Number(data.batch.version)} editable={editable} title="Supplier payment batch support" purposes={[{key:'payment_batch_support',label:'Batch support',description:'Attach the approved source schedule or beneficiary verification used for this payment batch revision.'}]}/>}</FinanceRecordWorkspace></FeaturePage>;
}
