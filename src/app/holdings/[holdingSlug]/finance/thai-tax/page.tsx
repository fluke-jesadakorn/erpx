import { RecordLink as Link } from "@/components/records/record-link";
import {FinanceWorkspace} from "@/components/finance/finance-workspace";
import {TaxIdentityConsole,ThaiMonthlyWithholdingConsole} from "@/components/finance/finance-thai-tax-console";
import {hasPermission} from "@/lib/permissions";
import {workspaceBase} from "@/lib/workspace";
import {publicScope,requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {taxIdentityWorkspace} from "@/server/modules/finance/public/finance-tax-identities";
import {withholdingWorkspace} from "@/server/modules/finance/public/finance-withholding";
import {withholdingMonthlyWorkspace} from "@/server/modules/finance/public/finance-withholding-remittance";
import {pageError} from "@/server/modules/work_projection/public/page-error";
import { Card } from "@/components/ui/foundation";
export const dynamic="force-dynamic";
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor));
 const [identities,withholding,monthly]=await Promise.all([taxIdentityWorkspace(actor),withholdingWorkspace(actor),withholdingMonthlyWorkspace(actor)]).catch(error=>pageError(error,actor.holdingSlug));
 return <FinanceWorkspace base={base} active="tax" title="Thai statutory identities and documents" description="Reviewed legal identities, ordinary supplier withholding and retained paper tax documents."><TaxIdentityConsole base={base} data={identities} userId={actor.userId} canPrepare={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")}/>
  <Card as="section" appearance="standard" className="ui-panel"><h2>Supplier withholding work</h2>{withholding.assessments.length?withholding.assessments.map(row=><p key={String(row.id)}><Link href={`${base}/finance/payments/${row.payment_id}`}>{String(row.payment_number)}</Link> · {String(row.status)} · {String(row.form_kind)}</p>):<p>Prepare an ordinary withholding assessment from a draft supplier payment.</p>}</Card>
  <ThaiMonthlyWithholdingConsole base={base} data={monthly} userId={actor.userId} canPrepare={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")} canPay={hasPermission(actor,"finance.pay")}/>
  <Card as="section" appearance="standard" className="ui-panel"><h2>Retained issued documents</h2>{withholding.documents.length?withholding.documents.map(row=><p key={String(row.id)}><Link href={`${base}/finance/thai-documents/${row.id}`}>{String(row.document_number)}</Link> · {String(row.issue_date)} · edition {Number(row.edition)}</p>):<p>No Thai tax documents have been issued in this Company.</p>}</Card>
 </FinanceWorkspace>;
}
