import { RecordLink as Link } from "@/components/records/record-link";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { hasPermission } from "@/lib/permissions";
import { minorToDecimal } from "@/lib/finance-money";
import { vatReviewLabels,vatTreatmentLabels } from "@/lib/finance-vat-labels";
import { taxEntryDetail } from "@/server/finance-tax-register";
import { pageError } from "@/server/page-error";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { VatCorrectionPreparation } from "@/components/finance/vat-register-controls";
import { Card } from "@/components/ui/foundation";

export const dynamic="force-dynamic";
export default async function TaxEntryPage({params}:{params:Promise<{holdingSlug:string;id:string}>}){
  const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),data=await taxEntryDetail(actor,id).catch(error=>pageError(error,holdingSlug)),entry=data.entry,evidence=JSON.parse(String(entry.evidence_json));
  const isCurrent=data.currentId===entry.id,canPrepare=hasPermission(actor,"finance.write")&&!actor.branchId&&!actor.departmentId;
  return <FinanceWorkspace base={base} active="tax" title={`VAT · ${entry.document_reference}`} description={isCurrent?"Current approved register entry":"Retained original evidence and review history"}>
    <Link href={`${base}/finance/tax?filingMonth=${entry.claim_month}&chart=${entry.chart_id}`}>Back to tax register</Link>
    {!isCurrent&&data.currentId&&<p className="alert">This entry is retained as history. <Link href={`${base}/finance/tax/entries/${data.currentId}`}>Open latest approved entry</Link></p>}
    <Card as="section" appearance="standard" className="ui-panel"><h2>Entry and retained evidence</h2><p>{vatReviewLabels[String(entry.status)]} · {entry.direction==="input"?"Input VAT":"Output VAT"} · {vatTreatmentLabels[String(entry.treatment)]}</p><p>Document date {String(entry.document_date)} · Accounting date {String(entry.accounting_date)} · Tax-point date {String(entry.tax_point_date)} · Claim month {String(entry.claim_month)}</p><p>Taxable {minorToDecimal(Number(entry.taxable_minor))} · VAT {minorToDecimal(Number(entry.tax_minor))} THB{entry.direction==="input"&&<> · Deductible {minorToDecimal(Number(entry.deductible_minor))} THB ({Number(entry.deductible_bps)/100}%)</>}</p><p>Tax rules and legal sources: {evidence.facts.legalBasis}</p><p>Tax-point evidence: {evidence.facts.taxPointEvidence}</p><p>Document evidence: {evidence.facts.documentEvidence}</p>{entry.review_reason&&<p>Review decision: {String(entry.review_reason)}</p>}<Link href={`${base}/finance/journals/${evidence.source.journal_id}`}>Open source journal</Link></Card>
    {isCurrent&&canPrepare&&Number(data.currentSourceVersion)>0&&<VatCorrectionPreparation base={base} entry={entry} currentSourceVersion={Number(data.currentSourceVersion)}/>}
    <Card as="section" appearance="standard" className="ui-panel"><h2>Complete entry history</h2><p className="muted">Original and replacement entries are retained separately. Exported and filed packages retain their original snapshot.</p>{data.history.map(row=><p key={String(row.id)}><Link href={`${base}/finance/tax/entries/${row.id}`}>{String(row.document_reference)} · {String(row.claim_month)} · {vatTreatmentLabels[String(row.treatment)]} · {vatReviewLabels[String(row.status)]}{row.id===data.currentId?" · Current":""}</Link>{row.correction_id&&<> · <Link href={`${base}/finance/tax/corrections/${row.correction_id}`}>Correction set</Link></>}</p>)}</Card>
  </FinanceWorkspace>;
}
