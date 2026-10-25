import { Table as StandardTable, TableFrame } from "@/components/ui/foundation";
import { RecordLink as Link } from "@/components/records/record-link";
import {FinanceWorkspace} from "@/components/finance/finance-workspace";
import { ThaiDocumentControls } from "@/components/records/loaders/finance--finance-thai-tax-console";
import {minorToDecimal} from "@/lib/finance-money";
import {hasPermission} from "@/lib/permissions";
import {workspaceBase} from "@/lib/workspace";
import {publicScope,requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {thaiDocumentDetail} from "@/server/modules/finance/public/finance-thai-documents";
import {pageError} from "@/server/modules/work_projection/public/page-error";
import { Card } from "@/components/ui/foundation";
export const dynamic="force-dynamic";
export default async function Page({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),data=await thaiDocumentDetail(actor,id).catch(error=>pageError(error,actor.holdingSlug)),snapshot=data.snapshot;
 return <FinanceWorkspace base={base} active="tax" title={data.document.document_number} description={`${snapshot.kind==="withholding_certificate"?"50 ทวิ · "+snapshot.formKind:"Thai VAT invoice"} · retained edition ${data.document.edition}`}>
  <p><Link href={`${base}/finance/thai-tax`}>Statutory identities and documents</Link></p><Card as="section" appearance="standard" className="ui-panel"><h2>Issued Thai document</h2><p>{snapshot.payer.legal_name} · {snapshot.payer.tax_id}</p><p>{snapshot.payee.legal_name} · {snapshot.payee.tax_id}</p><p>Issued {snapshot.issueDate}{snapshot.paymentDate?` · paid ${snapshot.paymentDate}`:""}</p><p>Income THB {minorToDecimal(snapshot.incomeMinor)} · tax THB {minorToDecimal(snapshot.taxMinor)} · gross source THB {minorToDecimal(snapshot.grossMinor)}</p>
   <TableFrame appearance="standard" label="Records"><StandardTable><thead><tr><th>Income / item</th><th>Income THB</th><th>Tax THB</th></tr></thead><tbody>{snapshot.items.map((item,index)=><tr key={index}><td>{item.description}</td><td>{minorToDecimal(item.incomeMinor)}</td><td>{minorToDecimal(item.taxMinor)}</td></tr>)}</tbody></StandardTable></TableFrame>
   <p><a href={`/api/v1${base}/finance/thai-documents/${id}/export`}>Download retained issued document</a></p><p className="muted" style={{overflowWrap:"anywhere"}}>SHA256 {data.document.export_sha256}</p>{data.document.previous_document_id&&<p><Link href={`${base}/finance/thai-documents/${data.document.previous_document_id}`}>Open original retained edition</Link></p>}
   {data.document.payment_id&&<Link href={`${base}/finance/payments/${data.document.payment_id}`}>Open source payment</Link>}{data.document.invoice_id&&<Link href={`${base}/finance/invoices/${data.document.invoice_id}`}>Open source invoice</Link>}
  </Card><Card as="section" appearance="standard" className="ui-panel"><h2>Document history</h2>{data.events.map(event=><p key={String(event.id)}>{String(event.action)} · {String(event.reason)} {Boolean(event.replacement_document_id)&&<Link href={`${base}/finance/thai-documents/${event.replacement_document_id}`}>Open replacement edition</Link>}</p>)}</Card>
  <ThaiDocumentControls base={base} data={data} userId={actor.userId} canPrepare={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")}/>
 </FinanceWorkspace>;
}
