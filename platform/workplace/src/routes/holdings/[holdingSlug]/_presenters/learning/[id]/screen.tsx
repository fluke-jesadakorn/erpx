import {learningBenefitContinuation} from "@erpx/learning-server/public/benefit-continuation";
import {LearningCertificateControls} from "@erpx/learning-ui/components/learning-certificate-controls";
import {learningConsoleSnapshot,learningPurchased} from "@erpx/learning-ui/lib/purchase-controls";
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { nativeLearningDetailPage } from "@erpx/platform-workplace/lib/native-learning";
import { LearningConsole } from "@erpx/records-ui/components/loaders/modules--learning-console";
import { RecordDocumentPanel } from "@erpx/records-ui/components/loaders/attachments--record-document-panel";
import { samePerson } from "@erpx/identity-access-ui/lib/iam";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { PageContent, Card } from "@erpx/platform-ui/components/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await nativeLearningDetailPage(a, id).catch((e) => pageError(e, holdingSlug)),
    x = data.assignment,
    course = data.courses.find((item) => item.id === x.course_id),
    canManageDocuments = hasPermission(a, "learning.documents.manage"),
    learner = samePerson(a, x.learner_id),
    benefits = await learningBenefitContinuation(a, data as unknown as Parameters<typeof learningBenefitContinuation>[1]);
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={canonicalUiHref(`${base}/learning`)}>Learning</Link>
      <h1>{String(x.course_title)}</h1>
      <p>
        {String(x.learner_name)} · due {String(x.due_date)} · {String(x.status)}
      </p>
      <LearningConsole base={base} detail={learningConsoleSnapshot(data as unknown as Parameters<typeof learningConsoleSnapshot>[0],true)} />
      {course && (
        <RecordDocumentPanel appearance="standard"
          base={base}
          targetType="learning_course"
          targetId={String(course.id)}
          expectedTargetVersion={Number(course.version)}
          editable={canManageDocuments&&learningPurchased(data.purchases,"learn.courses","newWork")}
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
      {String(x.status) === "completed" && learningPurchased(data.purchases,"learn.certificates","history") && (canManageDocuments || learner) && (
        <RecordDocumentPanel appearance="standard"
          base={base}
          targetType="learning_assignment"
          targetId={String(x.id)}
          expectedTargetVersion={Number(x.version)}
          editable={(canManageDocuments || learner)&&learningPurchased(data.purchases,"learn.certificates","settleExisting")}
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
      {data.certificate&&<LearningCertificateControls base={base} certificate={{id:String(data.certificate.id),revoked:!!data.certificate.revocation_id}} assignmentVersion={Number(x.version)} canReview={data.canReview&&data.canWrite} purchases={data.purchases}/>}
      <Card as="section" appearance="standard" className="ui-panel">
        <h2>History</h2>
        {data.history.map((e) => (
          <p key={String(e.id)}>
            Version {String(e.version)} · {String(e.action)} ·{" "}
            {String(e.reason)}
          </p>
        ))}
      </Card>
      {data.handoff && (
        <Card as="section" appearance="standard" className="ui-panel">
          <h2>Eligibility handoff</h2>
          <p>
            {String(data.handoff.eligibility_code)} · source version{" "}
            {String(data.handoff.source_assignment_version)}
          </p>
          {benefits&&<><Link href={canonicalUiHref(`${base}/benefits?view=enrollments`)}>Review your Benefits eligibility</Link><p>Benefits checks the current employee, enrollment window and retained certificate before creating coverage.</p></>}
        </Card>
      )}
    </PageContent>
  );
}
