"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Dialog } from "@/components/ui/dialog";
import { useState } from "react";
import { Button,Field,Select,TextArea,TextInput } from "@/components/ui/foundation";
import { decimalToMinor,minorToDecimal } from "@/lib/finance/finance-money";
import { thaiPayroll2026Pack } from "@/lib/people_payroll/thai-payroll-pack";
import { usePeopleCommand } from "@/components/people_payroll/payroll-controls";
import { Card, Stack, FormLayout } from "@/components/ui/foundation";
type Row=Record<string,unknown>;

export function PayrollStatutorySettings({base,packs,profiles,employees,canPrepare,canReview}:{base:string;packs:Row[];profiles:Row[];employees:Row[];canPrepare:boolean;canReview:boolean}){
  const {send,busy,error}=usePeopleCommand(base),[profileOpen,setProfileOpen]=useState(false),[reason,setReason]=useState("");
  const names=new Map(employees.map(e=>[String(e.id),String(e.legal_name)])),current=packs.filter(p=>p.adapter_id==="thai-payroll-v1"),hasRevision=current.some(p=>p.version===thaiPayroll2026Pack.rules.version&&p.status!=="superseded");
  return <Card as="section" appearance="standard" className="ui-panel ui-stack">
    <h2>Thai statutory payroll</h2>
    <p className="muted">Monthly employment income and ordinary Section 33 contributions. The source revision and employee declarations require independent review.</p>
    <p>Available revision: {thaiPayroll2026Pack.rules.version} · {thaiPayroll2026Pack.rules.effectiveFrom}–{thaiPayroll2026Pack.rules.effectiveTo}</p>
    <p className="muted">{thaiPayroll2026Pack.sources.map((s,i)=><span key={s.kind}>{i>0?" · ":""}<a href={s.url} target="_blank" rel="noreferrer">{s.kind.replaceAll("_"," ")}</a></span>)}</p>
    {canPrepare&&!hasRevision&&<Button appearance="standard" variant="secondary" disabled={busy} onClick={()=>void send("country-packs",{countryCode:"TH",version:thaiPayroll2026Pack.rules.version,rules:thaiPayroll2026Pack,exportFormats:["generic-csv"]})}>Prepare source revision</Button>}
    {canReview&&<Field appearance="standard">Review evidence<TextArea appearance="standard" value={reason} onChange={e=>setReason(e.target.value)} minLength={8} placeholder="Record the source/reference or declaration evidence reviewed"/></Field>}
    {current.map(p=><Card as="div" appearance="standard" key={String(p.id)} className="ui-panel">
      <strong>{String(p.version)}</strong><p>{String(p.status)} · revision {String(p.record_version)}</p>
      {canReview&&p.status==="testing"&&<Button appearance="standard" disabled={busy||reason.trim().length<8} onClick={()=>void send(`country-packs/${p.id}/transition`,{status:"validated",expectedVersion:Number(p.record_version),proof:reason})}>Validate source and reference</Button>}
      {canReview&&p.status==="validated"&&<Button appearance="standard" disabled={busy||reason.trim().length<8} onClick={()=>void send(`country-packs/${p.id}/transition`,{status:"active",expectedVersion:Number(p.record_version),proof:reason})}>Activate reviewed revision</Button>}
      {canReview&&p.status==="active"&&<Button appearance="standard" variant="secondary" disabled={busy||reason.trim().length<8} onClick={()=>void send(`country-packs/${p.id}/transition`,{status:"superseded",expectedVersion:Number(p.record_version),proof:reason})}>Supersede revision</Button>}
    </Card>)}
    <h3>Employee allowances and opening YTD</h3>
    {canPrepare&&<Button appearance="standard" variant="secondary" onClick={()=>setProfileOpen(!profileOpen)}>{profileOpen?"Close declaration form":"Prepare employee declaration"}</Button>}
    {profileOpen&&<Dialog appearance="standard" open={profileOpen} onClose={() => setProfileOpen(false)} title="Employee tax declaration" presentation="form-drawer" dismissDisabled={busy}><Stack as="div" className="ui-stack">{error && <InlineNotice tone="danger" role="alert" className="error">{error}</InlineNotice>}<FormLayout className="ui-form-grid" onSubmit={async e=>{
      e.preventDefault();const form=new FormData(e.currentTarget),employee=employees.find(p=>p.id===form.get("engagementId"));
      const value=(name:string)=>String(form.get(name)??"");
      if(await send("payroll/thai-profiles",{engagementId:value("engagementId"),expectedEngagementVersion:Number(employee?.version),taxYear:Number(value("taxYear")),effectiveFrom:value("effectiveFrom"),effectiveTo:value("effectiveTo"),verifiedAnnualDeductionsMinor:decimalToMinor(value("annualDeductions")),scheduledPaysInYear:Number(value("scheduledPays")),socialSecurityEligible:value("socialSecurityEligible")==="true",...(value("exemptionEvidence").trim()?{socialSecurityExemptionEvidence:value("exemptionEvidence")} :{}),openingThroughDate:value("openingThroughDate"),openingTaxableMinor:decimalToMinor(value("openingTaxable")),openingIrregularMinor:decimalToMinor(value("openingIrregular")),openingWithheldMinor:decimalToMinor(value("openingWithheld")),priorIncomeComplete:form.get("priorIncomeComplete")==="on",evidenceRef:value("evidenceRef")}))setProfileOpen(false);
    }}>
      <Field appearance="standard">Employee<Select appearance="standard" name="engagementId" required><option value="">Select employee</option>{employees.map(e=><option key={String(e.id)} value={String(e.id)}>{String(e.legal_name)} · {String(e.status)}</option>)}</Select></Field>
      <Field appearance="standard">Tax year<TextInput appearance="standard" name="taxYear" type="number" min="2000" max="2200" defaultValue="2026" required/></Field>
      <Field appearance="standard">Effective from<TextInput appearance="standard" name="effectiveFrom" type="date" defaultValue="2026-01-01" required/></Field>
      <Field appearance="standard">Effective to<TextInput appearance="standard" name="effectiveTo" type="date" defaultValue="2026-12-31" required/></Field>
      <Field appearance="standard">Verified annual allowances and deductions (THB)<TextInput appearance="standard" name="annualDeductions" type="number" min="0" step="0.01" required/></Field>
      <Field appearance="standard">Expected monthly pays this year<TextInput appearance="standard" name="scheduledPays" type="number" min="1" max="12" defaultValue="12" required/></Field>
      <Field appearance="standard">Section 33 eligibility<Select appearance="standard" name="socialSecurityEligible" defaultValue="true"><option value="true">Eligible</option><option value="false">Verified statutory exemption</option></Select></Field>
      <Field appearance="standard">Exemption evidence, when applicable<TextInput appearance="standard" name="exemptionEvidence" maxLength={2000}/></Field>
      <Field appearance="standard">Opening YTD through<TextInput appearance="standard" name="openingThroughDate" type="date" defaultValue="2025-12-31" required/></Field>
      <Field appearance="standard">Opening taxable income (THB)<TextInput appearance="standard" name="openingTaxable" type="number" min="0" step="0.01" defaultValue="0" required/></Field>
      <Field appearance="standard">Opening occasional income included above (THB)<TextInput appearance="standard" name="openingIrregular" type="number" min="0" step="0.01" defaultValue="0" required/></Field>
      <Field appearance="standard">Opening tax withheld (THB)<TextInput appearance="standard" name="openingWithheld" type="number" min="0" step="0.01" defaultValue="0" required/></Field>
      <Field appearance="standard"><label><input name="priorIncomeComplete" type="checkbox" required/> Prior income and withholding have been reconciled</label></Field>
      <Field appearance="standard">Declaration and opening YTD evidence<TextArea appearance="standard" name="evidenceRef" minLength={8} maxLength={2000} required/></Field>
      <Button appearance="standard" clientManaged type="submit" disabled={busy}>Save declaration for review</Button>
    </FormLayout></Stack></Dialog>}
    {profiles.map(p=><Card as="div" appearance="standard" key={String(p.id)} className="ui-panel">
      <strong>{names.get(String(p.engagement_id))??String(p.engagement_id)}</strong>
      <p>{String(p.tax_year)} · {String(p.effective_from)}–{String(p.effective_to)} · {String(p.status)} · revision {String(p.version)}</p>
      <p className="muted">Annual deductions THB {minorToDecimal(Number(p.annual_deductions_minor))}; opening taxable THB {minorToDecimal(Number(p.opening_taxable_minor))}; withholding THB {minorToDecimal(Number(p.opening_withheld_minor))} through {String(p.opening_through_date)}</p>
      <p>{String(p.evidence_ref)}</p>
      {canReview&&["draft","approved"].includes(String(p.status))&&<Button appearance="standard" variant="secondary" disabled={busy||reason.trim().length<8} onClick={()=>void send(`payroll/thai-profiles/${p.id}/decision`,{action:p.status==="draft"?"approve":"supersede",expectedVersion:Number(p.version),reason})}>{p.status==="draft"?"Approve reviewed declaration":"Supersede declaration"}</Button>}
    </Card>)}
    {error&&<InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
  </Card>;
}
