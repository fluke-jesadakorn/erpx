import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { learningWorkspace } from "@/server/learning";
import { LearningConsole } from "@/components/modules/learning-console";
import { PageContent, Card } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{view?:string}>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    personal = (await searchParams).view === "mine",
    data = await learningWorkspace(a,personal).catch((e) => pageError(e, holdingSlug));
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={`${base}/work${personal ? "?view=for-me" : ""}`}>My work</Link>
      <h1>{personal ? "My learning" : "Learning"}</h1>
      <p>
        {personal ? "Complete your assigned courses and follow the recorded results." : "Publish reviewed courses, assign current workers, record provider results and hand completed eligibility to the next owner."}
      </p>
      {!personal && <LearningConsole base={base} data={data} />}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>Assignments</h2>
        {!data.assignments.length && <p>No learning assignments are available in this view.</p>}
        {data.assignments.map((x) => (
          <Link
            className="ui-record-item"
            key={String(x.id)}
            href={`${base}/learning/${x.id}`}
          >
            {String(x.course_title)} · {String(x.learner_name)} ·{" "}
            {String(x.status)}
          </Link>
        ))}
      </Card>
    </PageContent>
  );
}
