import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { learningWorkspace } from "@/server/learning";
import { LearningConsole } from "@/components/modules/learning-console";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await learningWorkspace(a).catch((e) => pageError(e, holdingSlug));
  return (
    <div className="content-stack">
      <Link href={`${base}/work`}>Work</Link>
      <h1>Learning</h1>
      <p>
        Publish reviewed courses, assign current workers, record provider
        results and hand completed eligibility to the next owner.
      </p>
      <LearningConsole base={base} data={data} />
      <section className="card">
        <h2>Assignments</h2>
        {data.assignments.map((x) => (
          <Link
            className="row"
            key={String(x.id)}
            href={`${base}/learning/${x.id}`}
          >
            {String(x.course_title)} · {String(x.learner_name)} ·{" "}
            {String(x.status)}
          </Link>
        ))}
      </section>
    </div>
  );
}
