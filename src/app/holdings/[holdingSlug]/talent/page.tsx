import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { talentWorkspace } from "@/server/talent";
import { TalentConsole } from "@/components/modules/talent-console";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await talentWorkspace(a).catch((e) => pageError(e, holdingSlug));
  return (
    <div className="content-stack">
      <Link href={`${base}/work`}>Work</Link>
      <h1>Talent</h1>
      <p>
        Control private candidate data from approved requisition through
        reviewed offer and onboarding handoff.
      </p>
      <TalentConsole base={base} data={data} />
      <section className="card">
        <h2>Applications</h2>
        {data.applications.map((x) => (
          <Link
            className="row"
            key={String(x.id)}
            href={`${base}/talent/${x.id}`}
          >
            {String(x.candidate_name)} · {String(x.job_title)} ·{" "}
            {String(x.status)}
          </Link>
        ))}
      </section>
    </div>
  );
}
