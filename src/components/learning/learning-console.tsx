"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { FormDisclosure } from "@/components/ui/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work_projection/work-fetch";
import type { learningAssignmentDetail, learningWorkspace } from "@/server/modules/learning/public/learning";
import {
  AttachmentPanel,
  type AttachmentPanelChange,
} from "@/components/files/attachment-panel";
import { FormLayout } from "@/components/ui/foundation";
function useCommand(base: string) {
  const router = useRouter(),
    last = useRef<{ key: string; body: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body) last.current = { key: crypto.randomUUID(), body };
    setBusy(true);
    setError("");
    try {
      const response = await workFetch(`/api/v1${base}/learning`, {
          method: "POST",
          headers: {
            "Content-Type": "application/json",
            "Idempotency-Key": last.current.key,
          },
          body,
        }),
        result = await response.json();
      if (!response.ok) {
        if (response.status < 500) last.current = null;
        throw new Error(result.error?.message ?? "Unable to update learning.");
      }
      last.current = null;
      router.push(
        `${base}/learning${input && typeof input === "object" && "entity" in input && input.entity === "course" ? "" : `/${result.data.id}`}`,
      );
      router.refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Retry this action.");
    } finally {
      setBusy(false);
    }
  }
  return { send, busy, error };
}
export function LearningConsole({ recordOnly=false,
  base,
  data,
  detail,
}: { recordOnly?:boolean;
  base: string;
  data?: Awaited<ReturnType<typeof learningWorkspace>>;
  detail?: Awaited<ReturnType<typeof learningAssignmentDetail>>;
}) {
  const source = data ?? detail!,
    { send, busy, error } = useCommand(base),
    a = detail?.assignment,
    [completionFiles, setCompletionFiles] = useState<AttachmentPanelChange | null>(null);
  return (
    <>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {data?.canWrite && (
        (!recordOnly && <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Draft course</summary><FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget));
            void send({
              ...f,
              entity: "course",
              durationMinutes: Number(f.durationMinutes),
              requiredFor: f.requiredFor || null,
            });
          }}
        >
          <h2>Draft course</h2>
          <Field appearance="standard">
            Title
            <TextInput appearance="standard" name="title" required />
          </Field>
          <Field appearance="standard">
            Description
            <TextArea appearance="standard" name="description" required minLength={3} />
          </Field>
          <Field appearance="standard">
            Provider
            <TextInput appearance="standard" name="provider" required />
          </Field>
          <Field appearance="standard">
            Minutes
            <TextInput appearance="standard" name="durationMinutes" type="number" min="1" required />
          </Field>
          <Field appearance="standard">
            Eligibility code
            <TextInput appearance="standard" name="requiredFor" />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Draft course
          </Button>
        </FormLayout></FormDisclosure>)
      )}
      {data?.canReview &&
        data.courses
          .filter((x) => x.status === "draft")
          .map((x) => (
            <FormLayout
              className="ui-panel ui-form-grid"
              key={String(x.id)}
              onSubmit={(e) => {
                e.preventDefault();
                const f = Object.fromEntries(new FormData(e.currentTarget));
                void send({
                  ...f,
                  entity: "course",
                  id: x.id,
                  expectedVersion: Number(x.version),
                });
              }}
            >
              <h2>Review · {String(x.title)}</h2>
              <Field appearance="standard">
                Decision
                <Select appearance="standard" name="action">
                  <option value="publish">Publish</option>
                  <option value="reject">Reject</option>
                </Select>
              </Field>
              <Field appearance="standard">
                Reason
                <TextArea appearance="standard" name="reason" required minLength={3} />
              </Field>
              <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
                Record review
              </Button>
            </FormLayout>
          ))}
      {data?.canReview && (
        (!recordOnly && <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget)),
              course = data.courses.find((x) => x.id === f.courseId);
            void send({ ...f, courseVersion: Number(course?.version) });
          }}
        >
          <h2>Assign course</h2>
          <Field appearance="standard">
            Course
            <Select appearance="standard" name="courseId">
              {data.courses
                .filter((x) => x.status === "published")
                .map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.title)}
                  </option>
                ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Learner
            <Select appearance="standard" name="learnerId">
              {data.people.map((x) => (
                <option key={String(x.id)} value={String(x.id)}>
                  {String(x.name)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Due
            <TextInput appearance="standard" name="dueDate" type="date" required />
          </Field>
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Assign course
          </Button>
        </FormLayout>)
      )}
      {a && a.status === "assigned" && (
        (!recordOnly && <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            void send({
              id: a.id,
              action: "start",
              expectedVersion: Number(a.version),
              reason: String(new FormData(e.currentTarget).get("reason")),
            });
          }}
        >
          <h2>Start course</h2>
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Start
          </Button>
        </FormLayout>)
      )}
      {a && a.status === "in_progress" && (
        (!recordOnly && <FormDisclosure appearance="standard" className="ui-form-disclosure" dismissDisabled={busy}><summary>Record attendance and result</summary><FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget));
            void send({
              ...f,
              id: a.id,
              action: "complete",
              expectedVersion: Number(a.version),
              scoreBps: Number(f.scoreBps),
              ...(f.certificateValidFrom?{certificateValidFrom:f.certificateValidFrom}:{}),
              certificateExpiresOn:f.certificateExpiresOn||null,
              attachmentSetIds:
                completionFiles?.setId && completionFiles.readyCount > 0 && completionFiles.bindable
                  ? [completionFiles.setId]
                  : [],
            });
          }}
        >
          <h2>Record attendance and result</h2>
          <Field appearance="standard">
            Attendance
            <Select appearance="standard" name="attendance">
              <option value="attended">Attended</option>
              <option value="absent">Absent</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Score bps
            <TextInput appearance="standard" name="scoreBps" type="number" min="0" max="10000" required />
          </Field>
          <Field appearance="standard">
            Evidence
            <TextArea appearance="standard" name="evidence" required minLength={3} />
          </Field>
          <Field appearance="standard">Certificate valid from<TextInput appearance="standard" name="certificateValidFrom" type="date"/></Field>
          <Field appearance="standard">Certificate expiry, when applicable<TextInput appearance="standard" name="certificateExpiresOn" type="date"/></Field>
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <AttachmentPanel appearance="standard"
            base={base}
            targetType="learning_assignment"
            targetId={String(a.id)}
            expectedTargetVersion={Number(a.version)}
            componentKind="action"
            componentRef="completion"
            purposeKey="certificate"
            title="Certificate or completion evidence"
            description="Attach the provider certificate or other completion document. Clean files are locked with this result."
            visibility="Only the learner and authorized Learning document managers can access these files."
            onChange={setCompletionFiles}
          />
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Complete course
          </Button>
        </FormLayout></FormDisclosure>)
      )}
      {a && ["assigned", "in_progress"].includes(String(a.status)) && source.canReview && (
        (!recordOnly && <FormLayout
          className="ui-panel ui-form-grid"
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget));
            void send({ ...f, id: a.id, expectedVersion: Number(a.version) });
          }}
        >
          <h2>Reviewer exception</h2>
          <Field appearance="standard">
            Action
            <Select appearance="standard" name="action">
              <option value="waive">Waive</option>
              <option value="cancel">Cancel</option>
            </Select>
          </Field>
          <Field appearance="standard">
            Reason
            <TextArea appearance="standard" name="reason" required minLength={3} />
          </Field>
          <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy}>
            Record exception
          </Button>
        </FormLayout>)
      )}
    </>
  );
}
