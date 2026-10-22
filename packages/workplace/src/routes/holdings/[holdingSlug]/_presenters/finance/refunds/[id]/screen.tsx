import {financePurchaseControls} from "@erpx/server-finance/public/purchases";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceRecordWorkspace } from "@erpx/feature-records/components/loaders/finance--finance-record-workspace";
import {requireWorkContext,publicScope} from "@erpx/server-identity_access/public/authorization";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {refundWorkspace} from "@erpx/server-finance/public/refunds";
import { RefundConsole } from "@erpx/feature-records/components/loaders/modules--refund-console";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import { RecordDocumentPanel } from "@erpx/feature-records/components/loaders/attachments--record-document-panel";
import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
export const dynamic='force-dynamic';
export default async function RefundPage({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
 const data=await refundWorkspace(actor,id).catch(error=>pageError(error,holdingSlug));
 const capabilities=["finance.accounting","finance.banking","finance.close","finance.advanced","finance.receivables","finance.tax",...(data.sourcePurchase?[data.sourcePurchase.capability]:[])];
 const purchases=await financePurchaseControls(actor,capabilities);
 const canSettle=!purchases || !!purchases["finance.banking"]?.settleExisting || !!data.sourcePurchase?.settleExisting;
 return <FeaturePage className="ui-page-content" data-source="live"><FinanceRecordWorkspace sourceHistory={!!data.sourcePurchase && !purchases?.["finance.banking"]?.history} purchases={purchases} base={base} active="cash" title={`${data.owner.return_number} · Refund`} description={`${data.owner.party_name} · ${data.owner.currency} · ${data.owner.status}`}><RefundConsole key={`${actor.companyId}:${actor.assignmentId}:${id}`} base={base} data={data}/><RecordDocumentPanel appearance="standard" base={base} targetType="finance_refund_obligation" targetId={id} expectedTargetVersion={Number(data.owner.version)} editable={hasPermission(actor,'finance.pay')&&canSettle} title="Refund support" purposes={[{key:'refund_support',label:'Refund support',description:'Attach return correspondence, beneficiary confirmation, or other support for this refund obligation.'}]}/></FinanceRecordWorkspace></FeaturePage>;
}
