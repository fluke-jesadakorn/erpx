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
  searchParams: Promise<{ date?: string | string[] }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const summary = await leaveSummary(actor);
  const rawDate = (await searchParams).date;
  const date = typeof rawDate === "string" && z.iso.date().safeParse(rawDate).success ? rawDate : "";
  return (
    <main className={"content " + styles.leavePage}>
      <Link className={styles.backLink} href={base + "/leave"}>← My leave</Link>
      <header className={styles.pageHeading}>
        <div><p className={styles.eyebrow}>People · Self-service</p><h1>Request leave</h1><p>Choose your dates and review the policy calculation before submitting.</p></div>
      </header>
      {summary.setupMessage && <p className={styles.setupNotice} role="status">{summary.setupMessage}</p>}
      <LeaveEditor
        key={actor.companyId + ":" + actor.userId}
        base={base}
        types={summary.balances}
        initialDate={date}
      />
    </main>
  );
}
