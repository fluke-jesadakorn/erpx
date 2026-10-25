import Link from "next/link";
import { FeaturePage } from "@/components/features/feature-page";
import { PrintReceipt } from "@/components/finance/sales-controls";
import { Table, TableFrame, InlineNotice } from "@/components/ui/foundation";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { customerReceiptDetail } from "@/server/modules/finance/public/sales";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { workspaceBase } from "@/lib/workspace";
import { minorToDecimal } from "@/lib/finance-money";
import styles from "@/components/finance/sales-workspace.module.css";
export const dynamic = "force-dynamic";
export default async function ReceiptPage({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
  const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
  const {snapshot,status,sha256}=await customerReceiptDetail(actor,id).catch(error=>pageError(error,holdingSlug));
  return <FeaturePage className="ui-page-content" data-print-document><article className={styles.receipt}><div className={styles.noPrint}><Link href={`${base}/finance/sales?tab=receipts`}>Back to receipts</Link> · <PrintReceipt /> · <a target="_blank" rel="noreferrer" href={`${base.replace("/holdings/","/api/v1/holdings/")}/finance/sales/receipts/${id}/export`}>Open retained issued copy</a></div>
    {status === "cancelled" && <InlineNotice tone="danger">Cancelled — the source customer payment was reversed. This original issued receipt is retained for the audit history.</InlineNotice>}
    <header className={styles.receiptHeader}><div><h2>{snapshot.companyName}</h2><p>Received from<br /><strong>{snapshot.customerName}</strong><br />{snapshot.customerCode}</p></div><div><h1>Receipt</h1><p>{snapshot.documentNumber}<br />{snapshot.date}<br />{status}</p></div></header>
    <TableFrame appearance="standard" label="Receipt allocations"><Table><thead><tr><th>Invoice</th><th>Amount ({snapshot.currency})</th></tr></thead><tbody>{snapshot.allocations.map(row=><tr key={row.invoiceId}><td><Link href={`${base}/finance/invoices/${row.invoiceId}`}>{row.invoiceNumber}</Link></td><td>{minorToDecimal(Number(row.amountMinor))}</td></tr>)}</tbody></Table></TableFrame>
    <p className={styles.total}>Received <strong>{snapshot.currency} {minorToDecimal(Number(snapshot.amountMinor))}</strong></p><p>Payment reference: {snapshot.bankReference}</p><p>VAT documents are issued separately from this receipt.</p>
    <details className={styles.noPrint}><summary>Accounting & audit details</summary><p><Link href={`${base}/finance/payments/${snapshot.paymentId}`}>Source payment</Link> · <Link href={`${base}/finance/journals/${snapshot.journalId}`}>Posted journal</Link></p><p>Issued payment version {snapshot.paymentVersion} · Export SHA256 {sha256}</p></details>
  </article></FeaturePage>;
}
