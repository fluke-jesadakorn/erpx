import { FeaturePage } from "@/components/features/feature-page";
import { FinanceRecordWorkspace } from "@/components/finance/finance-record-workspace";
import {requireWorkContext,publicScope} from "@/server/modules/identity_access/public/authorization";
import {workspaceBase} from '@/lib/workspace';
import {refundWorkspace} from "@/server/modules/finance/public/refunds";
import {RefundConsole} from '@/components/modules/refund-console';
import {pageError} from "@/server/modules/work_projection/public/page-error";
import {RecordDocumentPanel} from '@/components/attachments/record-document-panel';
import {hasPermission} from '@/lib/permissions';
export const dynamic='force-dynamic';
export default async function RefundPage({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 const data=await refundWorkspace(actor,id).catch(error=>pageError(error,holdingSlug));
 return <FeaturePage className="ui-page-content" data-source="live"><FinanceRecordWorkspace base={base} active="cash" title={`${data.owner.return_number} · Refund`} description={`${data.owner.party_name} · ${data.owner.currency} · ${data.owner.status}`}><RefundConsole base={base} data={data}/><RecordDocumentPanel appearance="standard" base={base} targetType="finance_refund_obligation" targetId={id} expectedTargetVersion={Number(data.owner.version)} editable={hasPermission(actor,'finance.pay')} title="Refund support" purposes={[{key:'refund_support',label:'Refund support',description:'Attach return correspondence, beneficiary confirmation, or other support for this refund obligation.'}]}/></FinanceRecordWorkspace></FeaturePage>;
}
