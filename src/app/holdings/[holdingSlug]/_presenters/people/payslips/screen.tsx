import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { Table as StandardTable, TableFrame } from "@/components/ui/foundation";
import { RecordLink as Link } from "@/components/records/record-link";
import {FeaturePage} from "@/components/shared/features/feature-page";
import {PageHeader} from "@/components/ui/primitives";
import {minorToDecimal} from "@/lib/finance/finance-money";
import {workspaceBase} from "@/lib/work_projection/workspace";
import {publicScope,requireWorkContext} from "@/server/modules/identity_access/public/authorization";
import {pageError} from "@/server/modules/work_projection/public/page-error";
import {listPayslips} from "@/server/modules/people_payroll/public/payroll-payslips";
import { Card } from "@/components/ui/foundation";
export const dynamic="force-dynamic";
export default async function Payslips({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),slips=await listPayslips(actor).catch(error=>pageError(error,actor.holdingSlug));
 return <FeaturePage className="ui-page-content" data-source="live"><PageHeader appearance="standard" eyebrow="Your pay" title="My payslips" description="Retained employee copies from independently posted payroll. Each copy preserves its issued calculation revision."/>
  <p><Link href={canonicalUiHref(`${base}/people/payroll/recoveries`)}>My specific payroll recovery agreements</Link></p>
  {slips.length?<Card as="section" appearance="standard" className="ui-panel"><TableFrame appearance="standard" label="Records"><StandardTable><thead><tr><th>Pay date</th><th>Employee</th><th>Net payable</th><th>Issued copy</th></tr></thead><tbody>{slips.map(slip=><tr key={slip.id}><td>{slip.payDate}</td><td>{slip.employeeName}</td><td>{slip.currency} {minorToDecimal(slip.netMinor)}</td><td><Link href={canonicalUiHref(`${base}/people/payslips/${slip.id}`)}>Open payslip · revision {slip.runRevision}</Link></td></tr>)}</tbody></StandardTable></TableFrame></Card>:<p className="ui-panel">No issued employee payslips yet.</p>}
 </FeaturePage>;
}
