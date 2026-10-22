import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { InlineNotice } from "@erpx/ui/components/foundation";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { z } from "zod";
import { LeaveEditor } from "@erpx/feature-people_payroll/components/self-service/leave-editor";
import styles from "@erpx/feature-people_payroll/components/leave/leave-flow.module.css";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { leaveSummary } from "@erpx/server-people_payroll/public/leave-self-service";

import {workforcePresentationContext} from "@erpx/server-people_payroll/public/workforce-presentation";

export const dynamic = "force-dynamic";

export default async function NewLeave({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ date?: string | string[]; return?: string | string[] }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const summary = await leaveSummary(actor);
  const query=await searchParams,returnTo=typeof query.return==="string"?query.return:"";
  const rawDate = query.date;
  const date = typeof rawDate === "string" && z.iso.date().safeParse(rawDate).success ? rawDate : "";
  return (
    <div className={"content " + styles.leavePage}>
      <Link className={styles.backLink} href={returnTo?canonicalUiHref(`${base}/work?${new URLSearchParams(returnTo)}`):base+"/leave"}>← My leave</Link>
      <header className={styles.pageHeading}>
        <div><p className={styles.eyebrow}>People · Self-service</p><h1>Request leave</h1><p>Choose your dates and review the policy calculation before submitting.</p></div>
      </header>
      {summary.setupMessage && <InlineNotice tone="info" className={styles.setupNotice} role="status"><p>{summary.setupMessage}</p><p>HR must link your active engagement and assign a leave policy.</p><Link href={canonicalUiHref(`${base}/people`)}>Open People & HR setup →</Link></InlineNotice>}
      <LeaveEditor
        key={workforcePresentationContext(actor)}
        workContext={workforcePresentationContext(actor)}
        purchase={summary.purchase}
        base={base}
        companyId={actor.companyId}
        userId={actor.userId}
        assignmentId={actor.assignmentId}
        types={summary.balances}
        initialDate={date}
        returnTo={returnTo}
      />
    </div>
  );
}
