import {financeProductPresentationOptions} from "@/server/modules/finance/public/product-presentation-request";
import {financeActionAllowed} from "@/server/modules/finance/public/rules/purchase-controls";
import {financeDocumentPresentationAccess} from "@/server/modules/finance/public/record-presentation";
import {financePurchaseControls} from "@/server/modules/finance/public/purchases";
import { FeaturePage } from "@/components/shared/features/feature-page";
import { FinanceRecordWorkspace } from "@/components/records/loaders/finance--finance-record-workspace";
import {requireWorkContext,publicScope} from "@/server/modules/identity_access/public/authorization";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {supplierBatchWorkspace} from "@/server/modules/procurement/public/supplier-payments";
import { SupplierBatchConsole } from "@/components/records/loaders/modules--supplier-batch-console";
import {pageError} from "@/server/modules/work_projection/public/page-error";
import { RecordDocumentPanel } from "@/components/records/loaders/attachments--record-document-panel";
import {hasPermission} from "@/lib/identity_access/permissions";
export const dynamic='force-dynamic';
export default async function SupplierBatchPage({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 const data=await supplierBatchWorkspace(actor,id).catch(error=>pageError(error,holdingSlug)),purchases=await financePurchaseControls(actor),access=await financeDocumentPresentationAccess(actor,"supplier-batch",id,await financeProductPresentationOptions());
 const editable=hasPermission(actor,"finance.write")&&(financeActionAllowed(purchases,"settle-existing","finance.banking")||data.sourcePurchase?.settleExisting===true);
 return <FeaturePage className="ui-page-content" data-source="live"><FinanceRecordWorkspace sourceHistory={access.sourceOnly} purchases={purchases} base={base} active="cash" title="Supplier payment batch" description={`${data.batch?.planned_date} · ${data.batch?.currency} · ${data.batch?.status}`}><SupplierBatchConsole owner={{companyId:actor.companyId,userId:actor.userId,assignmentId:actor.assignmentId,branchId:actor.branchId,departmentId:actor.departmentId}} base={base} data={data}/>{data.batch&&<RecordDocumentPanel appearance="standard" base={base} targetType="finance_supplier_batch" targetId={id} expectedTargetVersion={Number(data.batch.version)} editable={editable} title="Supplier payment batch support" purposes={[{key:'payment_batch_support',label:'Batch support',description:'Attach the approved source schedule or beneficiary verification used for this payment batch revision.'}]}/>}</FinanceRecordWorkspace></FeaturePage>;
}
