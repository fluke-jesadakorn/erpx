import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { publicScope,requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { vatReviewLabels,vatTreatmentLabels } from "@erpx/feature-finance/lib/finance-vat-labels";
import { taxEntryDetail } from "@erpx/server-finance/public/finance-tax-register";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { FinanceWorkspace } from "@erpx/feature-finance/components/finance-workspace";
import { VatCorrectionPreparation } from "@erpx/feature-records/components/loaders/finance--vat-register-controls";
import { Card } from "@erpx/ui/components/foundation";

export const dynamic="force-dynamic";
export default async function TaxEntryPage({params}:{params:Promise<{holdingSlug:string;id:string}>}){
  const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),data=await taxEntryDetail(actor,id).catch(error=>pageError(error,holdingSlug)),entry=data.entry,evidence=JSON.parse(String(entry.evidence_json));
  const isCurrent=data.currentId===entry.id,canPrepare=hasPermission(actor,"finance.write")&&!actor.branchId&&!actor.departmentId;
  return <FinanceWorkspace actor={actor} base={base} active="tax" title={`VAT · ${entry.document_reference}`} description={isCurrent?"Current approved register entry":"Retained original evidence and review history"}>
    <Link href={canonicalUiHref(`${base}/finance/tax?filingMonth=${entry.claim_month}&chart=${entry.chart_id}`)}>Back to tax register</Link>
    {!isCurrent&&data.currentId&&<p className="alert">This entry is retained as history. <Link href={canonicalUiHref(`${base}/finance/tax/entries/${data.currentId}`)}>Open latest approved entry</Link></p>}
    <Card as="section" appearance="standard" className="ui-panel"><h2>Entry and retained evidence</h2><p>{vatReviewLabels[String(entry.status)]} · {entry.direction==="input"?"Input VAT":"Output VAT"} · {vatTreatmentLabels[String(entry.treatment)]}</p><p>Document date {String(entry.document_date)} · Accounting date {String(entry.accounting_date)} · Tax-point date {String(entry.tax_point_date)} · Claim month {String(entry.claim_month)}</p><p>Taxable {minorToDecimal(Number(entry.taxable_minor))} · VAT {minorToDecimal(Number(entry.tax_minor))} THB{entry.direction==="input"&&<> · Deductible {minorToDecimal(Number(entry.deductible_minor))} THB ({Number(entry.deductible_bps)/100}%)</>}</p><p>Tax rules and legal sources: {evidence.facts.legalBasis}</p><p>Tax-point evidence: {evidence.facts.taxPointEvidence}</p><p>Document evidence: {evidence.facts.documentEvidence}</p>{entry.review_reason&&<p>Review decision: {String(entry.review_reason)}</p>}<Link href={canonicalUiHref(`${base}/finance/journals/${evidence.source.journal_id}`)}>Open source journal</Link></Card>
    {isCurrent&&canPrepare&&Number(data.currentSourceVersion)>0&&<VatCorrectionPreparation base={base} entry={entry} currentSourceVersion={Number(data.currentSourceVersion)}/>}
    <Card as="section" appearance="standard" className="ui-panel"><h2>Complete entry history</h2><p className="muted">Original and replacement entries are retained separately. Exported and filed packages retain their original snapshot.</p>{data.history.map(row=><p key={String(row.id)}><Link href={canonicalUiHref(`${base}/finance/tax/entries/${row.id}`)}>{String(row.document_reference)} · {String(row.claim_month)} · {vatTreatmentLabels[String(row.treatment)]} · {vatReviewLabels[String(row.status)]}{row.id===data.currentId?" · Current":""}</Link>{row.correction_id&&<> · <Link href={canonicalUiHref(`${base}/finance/tax/corrections/${row.correction_id}`)}>Correction set</Link></>}</p>)}</Card>
  </FinanceWorkspace>;
}
