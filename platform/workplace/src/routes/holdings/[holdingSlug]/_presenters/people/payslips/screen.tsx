import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { Table as StandardTable, TableFrame } from "@erpx/platform-ui/components/foundation";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import {FeaturePage} from "@erpx/platform-shared-ui/components/features/feature-page";
import {PageHeader} from "@erpx/platform-ui/components/primitives";
import {minorToDecimal} from "@erpx/finance-ui/lib/finance-money";
import {workspaceBase} from "@erpx/work-projection-ui/lib/workspace";
import {publicScope,requireWorkContext} from "@erpx/identity-access-server/public/authorization";
import {pageError} from "@erpx/work-projection-server/public/page-error";
import {listPayslips} from "@erpx/people-payroll-server/public/payroll-payslips";
import { Card } from "@erpx/platform-ui/components/foundation";
export const dynamic="force-dynamic";
export default async function Payslips({params}:{params:Promise<{holdingSlug:string}>}){
 const {holdingSlug}=await params,actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor)),slips=await listPayslips(actor).catch(error=>pageError(error,actor.holdingSlug));
 return <FeaturePage className="ui-page-content" data-source="live"><PageHeader appearance="standard" eyebrow="Your pay" title="My payslips" description="Retained employee copies from independently posted payroll. Each copy preserves its issued calculation revision."/>
  <p><Link href={canonicalUiHref(`${base}/people/payroll/recoveries`)}>My specific payroll recovery agreements</Link></p>
  {slips.length?<Card as="section" appearance="standard" className="ui-panel"><TableFrame appearance="standard" label="Records"><StandardTable><thead><tr><th>Pay date</th><th>Employee</th><th>Net payable</th><th>Issued copy</th></tr></thead><tbody>{slips.map(slip=><tr key={slip.id}><td>{slip.payDate}</td><td>{slip.employeeName}</td><td>{slip.currency} {minorToDecimal(slip.netMinor)}</td><td><Link href={canonicalUiHref(`${base}/people/payslips/${slip.id}`)}>Open payslip · revision {slip.runRevision}</Link></td></tr>)}</tbody></StandardTable></TableFrame></Card>:<p className="ui-panel">No issued employee payslips yet.</p>}
 </FeaturePage>;
}
