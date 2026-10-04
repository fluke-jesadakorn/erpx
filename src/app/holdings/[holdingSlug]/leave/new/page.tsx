import { InlineNotice } from "@/components/ui/foundation";
import Link from "next/link";
import { z } from "zod";
import { LeaveEditor } from "@/components/self-service/leave-editor";
import styles from "@/components/leave/leave-flow.module.css";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { leaveSummary } from "@/server/leave-self-service";

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
      <Link className={styles.backLink} href={returnTo?`${base}/work?${new URLSearchParams(returnTo)}`:base+"/leave"}>← My leave</Link>
      <header className={styles.pageHeading}>
        <div><p className={styles.eyebrow}>People · Self-service</p><h1>Request leave</h1><p>Choose your dates and review the policy calculation before submitting.</p></div>
      </header>
      {summary.setupMessage && <InlineNotice tone="info" className={styles.setupNotice} role="status"><p>{summary.setupMessage}</p><p>HR must link your active engagement and assign a leave policy.</p><Link href={`${base}/people`}>Open People & HR setup →</Link></InlineNotice>}
      <LeaveEditor
        key={actor.companyId + ":" + actor.userId}
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
