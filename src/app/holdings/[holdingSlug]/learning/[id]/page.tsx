import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { learningAssignmentDetail } from "@/server/learning";
import { LearningConsole } from "@/components/modules/learning-console";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await learningAssignmentDetail(a, id).catch((e) =>
      pageError(e, holdingSlug),
    ),
    x = data.assignment;
  return (
    <div className="content-stack">
      <Link href={`${base}/learning`}>Learning</Link>
      <h1>{String(x.course_title)}</h1>
      <p>
        {String(x.learner_name)} · due {String(x.due_date)} · {String(x.status)}
      </p>
      <LearningConsole base={base} detail={data} />
      <section className="card">
        <h2>History</h2>
        {data.history.map((e) => (
          <p key={String(e.id)}>
            Version {String(e.version)} · {String(e.action)} ·{" "}
            {String(e.reason)}
          </p>
        ))}
      </section>
      {data.handoff && (
        <section className="card">
          <h2>Eligibility handoff</h2>
          <p>
            {String(data.handoff.eligibility_code)} · source version{" "}
            {String(data.handoff.source_assignment_version)}
          </p>
        </section>
      )}
    </div>
  );
}
