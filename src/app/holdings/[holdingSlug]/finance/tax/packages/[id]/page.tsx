import Link from "next/link";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { hasPermission } from "@/lib/permissions";
import { minorToDecimal } from "@/lib/finance-money";
import { vatPackageLabels,vatTreatmentLabels } from "@/lib/finance-vat-labels";
import { vatPackageDetail,type TaxRow } from "@/server/finance-tax-register";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { VatPackageControls } from "@/components/finance/vat-register-controls";

export const dynamic="force-dynamic";
export default async function VatPackagePage({params}:{params:Promise<{holdingSlug:string;id:string}>}){
  const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),data=await vatPackageDetail(actor,id),p=data.package,result=data.snapshot.result;
  return <FinanceWorkspace base={base} active="tax" title={`VAT workpaper (ภ.พ.30) · ${p.filing_month}`} description={`${p.filing_kind==="original"?"Original":"Additional"} · ${vatPackageLabels[String(p.status)]??p.status} · Version ${p.version}`}>
    <Link href={`${base}/finance/tax?filingMonth=${p.filing_month}&chart=${p.chart_id}`}>Back to tax register</Link>
    {p.export_hash&&<a className="button secondary" href={`${base.replace('/holdings/','/api/v1/holdings/')}/finance/vat-packages/${p.id}/export`}>Download retained workpaper</a>}
    {data.pendingCorrections>0&&<p role="status" className="card">VAT correction sets awaiting review: {data.pendingCorrections} sets. Review corrections before approving or exporting this workpaper.</p>}
    {!data.currentRegisterMatches&&<p role="status" className="card">The current register has changed. This package retains its original figures and export. {['filed','paid'].includes(String(p.status))?'Review differences and prepare an additional filing referencing this package if required.':'If not filed, retain this package and prepare a replacement. If the original was filed, record the actual filing receipt evidence.'}</p>}
    <section className="card"><h2>Figures from the reviewed register</h2><p>Output VAT {minorToDecimal(result.outputVatMinor)} · Input VAT {minorToDecimal(result.inputVatMinor)} · Deductible {minorToDecimal(result.deductibleInputMinor)} THB</p><p>Net before carried credits and form adjustments {minorToDecimal(result.netMinor)} THB</p><p className="muted">This workpaper supports manual filing and has not been validated for Revenue Department import. Additional filings require comparison with prior filed figures and official form requirements.</p>{p.filing_reference&&<p>Filing receipt reference {String(p.filing_reference)}</p>}</section>
    {p.original_package_id&&data.snapshot.priorFiledResult&&<section className="card"><h2>Referenced original filing</h2><Link href={`${base}/finance/tax/packages/${p.original_package_id}`}>Open original package and receipt evidence</Link><p>Prior figures: Output VAT {minorToDecimal(data.snapshot.priorFiledResult.outputVatMinor)} · Input VAT {minorToDecimal(data.snapshot.priorFiledResult.inputVatMinor)} · Deductible {minorToDecimal(data.snapshot.priorFiledResult.deductibleInputMinor)} THB</p><p className="muted">Prior and current figures are retained separately. The responsible accountant must calculate additional filing figures using the official form requirements.</p></section>}
    {hasPermission(actor,"finance.write")&&!actor.branchId&&!actor.departmentId&&['prepared','approved_for_export','exported'].includes(String(p.status))&&<VatPackageControls base={base} package={p} canApprove={hasPermission(actor,"finance.approve")} userId={actor.userId} currentReady={data.currentRegisterMatches&&data.pendingCorrections===0}/>}
    <section className="card"><h2>Retained package entries</h2><div className="business-table-wrap" tabIndex={0} role="region" aria-label="Financial table — scroll for more columns"><table className="business-table"><thead><tr><th>Document</th><th>Tax-point date</th><th>Type</th><th>Taxable amount</th><th>VAT</th><th>Reviewer</th></tr></thead><tbody>{data.snapshot.register.map((e:TaxRow)=><tr key={String(e.id)}><td><Link href={`${base}/finance/tax/entries/${e.id}`}>{String(e.document_reference)}</Link></td><td>{String(e.tax_point_date)}</td><td>{vatTreatmentLabels[String(e.treatment)]??String(e.treatment)}</td><td>{minorToDecimal(Number(e.taxable_minor))}</td><td>{minorToDecimal(Number(e.tax_minor))}</td><td>{String(e.review_reason)}</td></tr>)}</tbody></table></div></section>
    <section className="card"><h2>Package history and evidence</h2>{data.events.map(e=>{
      const evidence=JSON.parse(String(e.evidence_json)),label=({prepare:"Prepare",approve:"Approve",export:"Export",file:"Record filing",supersede:"Retained as history"} as Record<string,string>)[String(e.action)]??String(e.action);
      return <details key={String(e.id)}><summary>Version {Number(e.version)} · {label}</summary><p>{evidence.reason}</p>{evidence.filingReference&&<p>Filing receipt reference: {evidence.filingReference} · Filed date: {evidence.filedDate}</p>}{evidence.receiptEvidence&&<p>Receipt evidence: {evidence.receiptEvidence}</p>}</details>;
    })}</section>
  </FinanceWorkspace>;
}
