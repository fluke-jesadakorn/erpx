import type { WorkContext } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { matchedBillLines } from "@/server/supplier-bill-matching";
import { decimalToMinor } from "@/lib/finance-money";
import { minorToDecimal } from "@/lib/finance-money";
type Row = Record<string,string|number|null>;
export async function DocumentPostingPreview({actor,kind,id}:{actor:WorkContext;kind:"invoice"|"stock_count";id:string}) {
  const {write}=await holdingResources(actor.holdingId);
  const row=await write.prepare(`SELECT * FROM ${kind === "invoice" ? "finance_invoices" : "inventory_counts"} WHERE company_id=? AND id=?`).bind(actor.companyId,id).first<Row>();
  if (!row) return null;
  if(kind==="invoice"&&row.status==="voided")return <section className="card"><h2>Voided draft</h2><p>This draft was not posted. Review its source document for the correction history.</p></section>;
  const accounts=(await write.prepare("SELECT id,code,name,status FROM finance_accounts WHERE company_id=?").bind(actor.companyId).all<Row>()).results;
  let lines:{id:string;debit:number;credit:number}[]=[];
  if (row.journal_id) {
    lines=(await write.prepare("SELECT account_id id,debit_minor debit,credit_minor credit FROM finance_journal_lines WHERE journal_id=? ORDER BY ordinal").bind(row.journal_id).all<{id:string;debit:number;credit:number}>()).results;
  } else if (kind === "invoice") {
    const payable=row.kind==="payable",net=Number(row.subtotal_minor),tax=Number(row.tax_minor),total=Number(row.total_minor);
    lines=[{id:String(row.debit_account_id),debit:payable?net:total,credit:0},{id:String(row.credit_account_id),debit:0,credit:payable?total:net},...(tax?[{id:String(row.tax_account_id),debit:payable?tax:0,credit:payable?0:tax}]:[])];
    const matched=await write.prepare("SELECT * FROM procurement_bill_matches WHERE invoice_id=? AND company_id=? AND status='invoiced'").bind(id,actor.companyId).first<Row>();
    if(matched)lines=matchedBillLines(matched).map(l=>({id:l.accountId,debit:decimalToMinor(l.debit),credit:decimalToMinor(l.credit)}));
  } else {
    const config=await write.prepare("SELECT s.inventory_account_id,a.id variance_account_id FROM commerce_settings s JOIN finance_accounts a ON a.chart_id=s.chart_id AND a.company_id=s.company_id AND a.code='5400' AND a.category='expense' AND a.status='active' WHERE s.company_id=?").bind(actor.companyId).first<{inventory_account_id:string;variance_account_id:string}>();
    if (!config) return <p className="notice">Configure inventory and variance account 5400 before GL posting.</p>;
    const deltas=(await write.prepare("SELECT counted_value_minor-snapshot_value_minor delta FROM inventory_count_lines WHERE count_id=?").bind(id).all<{delta:number}>()).results;
    const gain=deltas.reduce((v,l)=>v+Math.max(l.delta,0),0),loss=deltas.reduce((v,l)=>v+Math.max(-l.delta,0),0);
    lines=[...(gain?[{id:config.inventory_account_id,debit:gain,credit:0},{id:config.variance_account_id,debit:0,credit:gain}]:[]),...(loss?[{id:config.variance_account_id,debit:loss,credit:0},{id:config.inventory_account_id,debit:0,credit:loss}]:[])];
  }
  const debit=lines.reduce((s,l)=>s+l.debit,0),credit=lines.reduce((s,l)=>s+l.credit,0);
  return <section className="card"><h2>{row.journal_id ? "Posted GL" : "GL preview"} · Debit / Credit</h2>{!lines.length?<p>No value adjustment; no GL journal is required.</p>:<table className="business-table"><thead><tr><th>Account</th><th>Debit</th><th>Credit</th></tr></thead><tbody>{lines.map((l,i)=>{const account=accounts.find(a=>a.id===l.id);return <tr key={i}><td>{account?`${account.code} · ${account.name}`:"Missing account mapping"}</td><td>{minorToDecimal(l.debit)}</td><td>{minorToDecimal(l.credit)}</td></tr>;})}</tbody><tfoot><tr><th>Total</th><td>{minorToDecimal(debit)}</td><td>{minorToDecimal(credit)}</td></tr></tfoot></table>}{debit!==credit&&<p className="error">Debit and Credit do not balance. Posting is blocked.</p>}</section>;
}
