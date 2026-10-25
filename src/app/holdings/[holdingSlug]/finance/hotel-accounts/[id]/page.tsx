import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { guestAccountFinanceEvidence } from "@/server/modules/pms/public/guest-account-finance-evidence";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { workspaceBase } from "@/lib/workspace";
import { MoneyAmount } from "@/components/money/currency-provider";
import { RecordLink as Link } from "@/components/records/record-link";
import { Table, TableFrame } from "@/components/ui/foundation";
export const dynamic="force-dynamic";
export default async function GuestAccountEvidence({params}:{params:Promise<{holdingSlug:string;id:string}>}) {
  const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
  const data=await guestAccountFinanceEvidence(actor,id).catch(error=>pageError(error,holdingSlug));
  return <FinanceWorkspace base={base} active="hotels" title="บัญชีผู้เข้าพัก / Guest Account" description={`${data.hotel} · ${data.businessDate}`}>
    <p>{data.account.name} · {data.account.state} · <MoneyAmount units={data.account.due} currency={data.currency} legacyScale={2}/></p>
    <TableFrame label="Guest Account financial evidence"><Table><thead><tr><th>Date</th><th>Description</th><th>Kind</th><th>Amount</th><th>Journal</th></tr></thead><tbody>{data.entries.map(row=><tr key={row.id}><td>{row.data.date}</td><td>{row.data.description}</td><td>{row.data.kind}</td><td><MoneyAmount units={row.data.total} currency={data.currency} legacyScale={2}/></td><td>{row.journalId?<Link href={`${base}/finance/accounting?journal=${row.journalId}`}>Posted journal</Link>:"No posting"}</td></tr>)}</tbody></Table></TableFrame>
  </FinanceWorkspace>;
}
