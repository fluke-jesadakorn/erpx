import { FeaturePage } from "@/components/features/feature-page";
import { FinanceRecordWorkspace } from "@/components/finance/finance-record-workspace";
import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {supplierBatchWorkspace} from '@/server/supplier-payments';
import {SupplierBatchConsole} from '@/components/modules/supplier-batch-console';
import {pageError} from '@/server/page-error';
import {RecordDocumentPanel} from '@/components/attachments/record-document-panel';
import {hasPermission} from '@/lib/permissions';
export const dynamic='force-dynamic';
export default async function SupplierBatchPage({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 const data=await supplierBatchWorkspace(actor,id).catch(error=>pageError(error,holdingSlug));
 return <FeaturePage className="ui-page-content" data-source="live"><FinanceRecordWorkspace base={base} active="cash" title="Supplier payment batch" description={`${data.batch?.planned_date} · ${data.batch?.currency} · ${data.batch?.status}`}><SupplierBatchConsole base={base} data={data}/>{data.batch&&<RecordDocumentPanel appearance="standard" base={base} targetType="finance_supplier_batch" targetId={id} expectedTargetVersion={Number(data.batch.version)} editable={hasPermission(actor,'finance.write')} title="Supplier payment batch support" purposes={[{key:'payment_batch_support',label:'Batch support',description:'Attach the approved source schedule or beneficiary verification used for this payment batch revision.'}]}/>}</FinanceRecordWorkspace></FeaturePage>;
}
