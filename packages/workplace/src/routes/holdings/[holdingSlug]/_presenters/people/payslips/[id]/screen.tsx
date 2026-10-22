import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { Table as StandardTable, TableFrame } from "@erpx/ui/components/foundation";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import {FeaturePage} from "@erpx/feature-shared/components/features/feature-page";
import {PageHeader} from "@erpx/ui/components/primitives";
import {minorToDecimal} from "@erpx/feature-finance/lib/finance-money";
import {workspaceBase} from "@erpx/feature-work_projection/lib/workspace";
import {publicScope,requireWorkContext} from "@erpx/server-identity_access/public/authorization";
import {pageError} from "@erpx/server-work_projection/public/page-error";
import {getPayslip} from "@erpx/server-people_payroll/public/payroll-payslips";
import { Card } from "@erpx/ui/components/foundation";
export const dynamic="force-dynamic";
export default async function Payslip({params}:{params:Promise<{holdingSlug:string;id:string}>}){
 const {holdingSlug,id}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),data=await getPayslip(actor,id).catch(error=>pageError(error,actor.holdingSlug)),s=data.snapshot;
 const statutory=s.result.statutory as {adapterId?:string;sourceDigest?:string;input?:Record<string,unknown>}|undefined,explanation=s.result.explanation as Record<string,unknown>|undefined,components=Array.isArray(explanation?.components)?explanation.components as Record<string,unknown>[]:[];
 return <FeaturePage className="ui-page-content" data-source="live"><PageHeader appearance="standard" eyebrow="Retained employee copy" title="Employee payslip" description={`${s.employee.name} · pay date ${s.run.payDate} · revision ${s.run.revision}`}/><p><Link href={canonicalUiHref(`${base}/people/payslips`)}>My payslips</Link></p>
  <Card as="section" appearance="standard" className="ui-panel"><h2>{s.company.name}</h2><p>{s.employee.jobTitle} · {s.run.periodStart} to {s.run.periodEnd}</p><dl><dt>Gross pay</dt><dd>{s.run.currency} {minorToDecimal(s.grossMinor)}</dd><dt>Employee deductions</dt><dd>{s.run.currency} {minorToDecimal(s.deductionsMinor)}</dd><dt>Net payable</dt><dd>{s.run.currency} {minorToDecimal(s.netMinor)}</dd></dl>
   <TableFrame appearance="standard" label="Records"><StandardTable><thead><tr><th>Component</th><th>Type</th><th>Amount</th></tr></thead><tbody>{components.filter(row=>row.includedInTotals!==false).map((row,index)=><tr key={index}><td>{String(row.label)}</td><td>{String(row.kind)}</td><td>{s.run.currency} {minorToDecimal(Number(row.amountMinor))}</td></tr>)}</tbody></StandardTable></TableFrame>
   <p>{s.statutory?`Dated Thai payroll calculation · ${statutory?.adapterId}. Reviewed employee evidence and engineering calculation checks; external certification is reported separately.`:`Company payroll calculation · ${s.run.mode}.`}</p>
   <p>Bank delivery: {String(data.payment?.status??"not prepared")}. The retained slip records the posted payable; the live payment record shows delivery separately.</p>
   <p><a href={`/api/v1${base}/people/payroll/payslips/${id}/export`}>Download issued payslip</a></p>
  </Card><details className="ui-panel"><summary>Issued revision and source evidence</summary><p>Run revision {s.run.revision} · source journal {s.run.journalId}</p><p>Source digest {s.sourceDigest}</p><p>Issued copy SHA256 {data.checksum}</p>{Boolean(statutory?.sourceDigest)&&<p>Thai rule digest {statutory?.sourceDigest}</p>}</details>
 </FeaturePage>;
}
