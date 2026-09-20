import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { talentDetail } from "@/server/talent";
import { TalentConsole } from "@/components/modules/talent-console";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await talentDetail(a, id).catch((e) => pageError(e, holdingSlug)),
    x = data.application;
  return (
    <div className="content-stack">
      <Link href={`${base}/talent`}>Talent</Link>
      <h1>{String(x.candidate_name)}</h1>
      <p>
        {String(x.job_title)} · {String(x.status)} · private until{" "}
        {String(x.retention_until)}
      </p>
      <TalentConsole base={base} detail={data} />
      <section className="card">
        <h2>Application history</h2>
        {data.history.map((h) => (
          <p key={String(h.id)}>
            Version {String(h.version)} · {String(h.action)} ·{" "}
            {String(h.reason)}
          </p>
        ))}
      </section>
      {data.handoff && (
        <section className="card">
          <h2>Onboarding handoff</h2>
          <p>
            Open · accepted offer version{" "}
            {String(data.handoff.source_offer_version)}
          </p>
        </section>
      )}
    </div>
  );
}
