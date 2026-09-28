import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { learningAssignmentDetail } from "@/server/learning";
import { LearningConsole } from "@/components/modules/learning-console";
import { RecordDocumentPanel } from "@/components/attachments/record-document-panel";
import { samePerson } from "@/lib/iam";
import { hasPermission } from "@/lib/permissions";
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
    x = data.assignment,
    course = data.courses.find((item) => item.id === x.course_id),
    canManageDocuments = hasPermission(a, "learning.documents.manage"),
    learner = samePerson(a, x.learner_id);
  return (
    <div className="content-stack">
      <Link href={`${base}/learning`}>Learning</Link>
      <h1>{String(x.course_title)}</h1>
      <p>
        {String(x.learner_name)} · due {String(x.due_date)} · {String(x.status)}
      </p>
      <LearningConsole base={base} detail={data} />
      {course && (
        <RecordDocumentPanel
          base={base}
          targetType="learning_course"
          targetId={String(course.id)}
          expectedTargetVersion={Number(course.version)}
          editable={canManageDocuments}
          title="Course material"
          purposes={[
            {
              key: "course_material",
              label: "Controlled course material",
              description:
                "Attach the current workbook, instructions, or provider material for this course revision.",
              visibility:
                "Assigned learners and authorized Learning document managers can access these files.",
            },
          ]}
        />
      )}
      {String(x.status) === "completed" && (canManageDocuments || learner) && (
        <RecordDocumentPanel
          base={base}
          targetType="learning_assignment"
          targetId={String(x.id)}
          expectedTargetVersion={Number(x.version)}
          editable={canManageDocuments || learner}
          title="Completion certificate"
          purposes={[
            {
              key: "certificate",
              label: "Certificate",
              description:
                "Attach a later certificate revision without changing the pinned completion evidence.",
              visibility:
                "Only the learner and authorized Learning document managers can access this certificate.",
            },
          ]}
        />
      )}
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
