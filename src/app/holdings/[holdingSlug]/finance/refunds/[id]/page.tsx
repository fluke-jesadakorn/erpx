import {requireWorkContext,publicScope} from '@/server/authorization';
import {workspaceBase} from '@/lib/workspace';
import {refundWorkspace} from '@/server/refunds';
import {RefundConsole} from '@/components/modules/refund-console';
import {pageError} from '@/server/page-error';
import {RecordDocumentPanel} from '@/components/attachments/record-document-panel';
import {hasPermission} from '@/lib/permissions';
export const dynamic='force-dynamic';
export default async function RefundPage({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 const data=await refundWorkspace(actor,id).catch(error=>pageError(error,holdingSlug));
 return <div className="content-stack"><RefundConsole base={base} data={data}/><RecordDocumentPanel base={base} targetType="finance_refund_obligation" targetId={id} expectedTargetVersion={Number(data.owner.version)} editable={hasPermission(actor,'finance.pay')} title="Refund support" purposes={[{key:'refund_support',label:'Refund support',description:'Attach return correspondence, beneficiary confirmation, or other support for this refund obligation.'}]}/></div>;
}
