import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import {FinanceWorkspace} from "@erpx/finance-ui/components/finance-workspace";
import {financeStyles} from "@erpx/finance-ui/components/finance-workspace";
import {FinancePurchaseProvider} from "@erpx/finance-ui/components/purchase-context";
import {TaxIdentityConsole,ThaiMonthlyWithholdingConsole} from "@erpx/finance-ui/components/finance-thai-tax-console";
import {hasPermission} from "@erpx/identity-access-ui/lib/permissions";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {publicScope,requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {financeLegalSetupWorkspace} from "@erpx/finance-server/public/legal-setup";
import {financeProductPresentationOptions} from "@erpx/finance-server/public/product-presentation-request";
import {pageError} from "@erpx/work-projection-server/public/page-error";
import { Card } from "@erpx/platform-ui/components/foundation";
export const dynamic="force-dynamic";
export default async function Page({params}:{params:Promise<{holdingSlug:string}>}){
 const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor));
 const data=await financeLegalSetupWorkspace(actor,await financeProductPresentationOptions()).catch(error=>pageError(error,actor.holdingSlug));
 const identityConsole=<TaxIdentityConsole base={base} data={data.identities} configurationCapabilities={data.configurationCapabilities} userId={actor.userId} canPrepare={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")}/>;
 if(data.sourceOnly||!data.tax)return <FinancePurchaseProvider purchases={data.purchases}><section className={financeStyles.shell}><header className={financeStyles.header}><div><p className="eyebrow">Company legal setup</p><h1>Thai statutory identities</h1><p>Maintain independently reviewed Company and legal party identities for product business documents.</p></div></header>{identityConsole}</section></FinancePurchaseProvider>;
 const {withholding,monthly}=data.tax;
 return <FinanceWorkspace actor={actor} base={base} active="tax" title="Thai statutory identities and documents" description="Reviewed legal identities, ordinary supplier withholding and retained paper tax documents.">{identityConsole}
  <Card as="section" appearance="standard" className="ui-panel"><h2>Supplier withholding work</h2>{withholding.assessments.length?withholding.assessments.map(row=><p key={String(row.id)}><Link href={canonicalUiHref(`${base}/finance/payments/${row.payment_id}`)}>{String(row.payment_number)}</Link> · {String(row.status)} · {String(row.form_kind)}</p>):<p>Prepare an ordinary withholding assessment from a draft supplier payment.</p>}</Card>
  <ThaiMonthlyWithholdingConsole base={base} data={monthly} userId={actor.userId} canPrepare={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")} canPay={hasPermission(actor,"finance.pay")}/>
  <Card as="section" appearance="standard" className="ui-panel"><h2>Retained issued documents</h2>{withholding.documents.length?withholding.documents.map(row=><p key={String(row.id)}><Link href={canonicalUiHref(`${base}/finance/thai-documents/${row.id}`)}>{String(row.document_number)}</Link> · {String(row.issue_date)} · edition {Number(row.edition)}</p>):<p>No Thai tax documents have been issued in this Company.</p>}</Card>
 </FinanceWorkspace>;
}
