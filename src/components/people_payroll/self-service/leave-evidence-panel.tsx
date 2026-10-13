"use client";

import { AttachmentPanel } from "@/components/files/attachment-panel";
import type { LeaveEvidencePolicy, LeaveEvidencePurpose } from "@/server/modules/people_payroll/public/leave-evidence";
import styles from "@/components/people_payroll/leave/leave-flow.module.css";
import { Stack } from "@/components/ui/foundation";

export type LeaveEvidenceFile = {
  id: string;
  set_id: string | null;
  purpose_key: string;
  original_name: string;
  bytes: number | null;
  media_type: string | null;
  source_state: string | null;
  link_state: string;
  preview_available: boolean;
  created_at: number;
};

const purposes: Array<{
  key: LeaveEvidencePurpose;
  title: string;
  description: string;
}> = [
  {
    key: "medical_certificate",
    title: "Medical certificates",
    description: "Attach the medical certificate required by the applicable Leave policy. Only clean files under this exact purpose count toward the requirement.",
  },
  {
    key: "leave_supporting_photo",
    title: "Supporting photos and documents",
    description: "Attach relevant photos or supporting documents. These do not satisfy a medical-certificate requirement.",
  },
];

const labels: Record<LeaveEvidencePurpose, string> = {
  medical_certificate: "Medical certificate",
  leave_supporting_photo: "Supporting photo or document",
};

export function LeaveEvidencePanel({
  base,
  requestId,
  requestVersion,
  evidencePolicy,
  initialFiles,
  canWrite,
  enabled,
}: {
  base: string;
  requestId: string;
  requestVersion: number;
  evidencePolicy: LeaveEvidencePolicy | null;
  initialFiles: LeaveEvidenceFile[];
  canWrite: boolean;
  enabled: boolean;
}) {
  const availableRequired = evidencePolicy?.purposeKey
    ? new Set(initialFiles
        .filter((file) => file.purpose_key === evidencePolicy.purposeKey && file.source_state === "available")
        .map((file) => file.id)).size
    : 0;
  const api = `/api/v1${base}`;

  return <section className={styles.attachmentPanel} aria-labelledby="leave-evidence-title">
    <div className={styles.attachmentHeading}>
      <div>
        <p className={styles.eyebrow}>Private evidence</p>
        <h2 id="leave-evidence-title">Medical certificates and supporting files</h2>
        <p>Visible only to you, the current eligible reviewer, reviewers who recorded a decision, and authorized HR.</p>
      </div>
      {evidencePolicy?.required && <span className={styles.attachmentRequirement}>
        {availableRequired}/{evidencePolicy.minimumCount} required
      </span>}
    </div>

    {evidencePolicy?.required && <p className={styles.detailMessage}>
      <strong>{labels[evidencePolicy.purposeKey!]}</strong> is required for {evidencePolicy.minimumUnits} or more leave units.
      {evidencePolicy.dueStage === "before_final_approval" ? " You may submit now, but final approval waits for the current clean manifest." : " Attach it before submitting."}
      {evidencePolicy.graceDays ? ` Policy due date is ${evidencePolicy.graceDays} day(s) after submission.` : ""}
      {evidencePolicy.note ? ` ${evidencePolicy.note}` : ""}
    </p>}

    {enabled ? <Stack as="div" className="ui-stack">
      {purposes.map((purpose) => <AttachmentPanel appearance="standard"
        key={`${requestId}:${requestVersion}:${purpose.key}`}
        base={base}
        targetType="leave_request"
        targetId={requestId}
        expectedTargetVersion={requestVersion}
        componentKind="record"
        componentRef={`leave-evidence:${purpose.key}`}
        purposeKey={purpose.key}
        title={purpose.title}
        description={purpose.description}
        visibility="Restricted to the requester, the current eligible reviewer, recorded decision makers, and holders of leave.evidence.read."
        readOnly={!canWrite}
        showHistory
      />)}
    </Stack> : <div className={styles.attachmentFiles}>
      {initialFiles.length ? initialFiles.map((file) => <article key={file.id}>
        <div>
          <strong>{file.original_name}</strong>
          <span>{labels[file.purpose_key as LeaveEvidencePurpose] ?? file.purpose_key.replaceAll("_", " ")} · legacy private evidence</span>
        </div>
        <div className={styles.attachmentActions}>
          <a href={`${api}/attachments/${file.id}/content`}>Download</a>
        </div>
      </article>) : <p>No leave evidence is attached.</p>}
    </div>}
  </section>;
}
