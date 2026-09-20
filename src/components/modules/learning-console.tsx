"use client";
import { useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { workFetch } from "@/lib/work-fetch";
import type {
  learningAssignmentDetail,
  learningWorkspace,
} from "@/server/learning";
function useCommand(base: string) {
  const router = useRouter(),
    last = useRef<{ key: string; body: string } | null>(null),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  async function send(input: unknown) {
    const body = JSON.stringify(input);
    if (last.current?.body !== body)
      last.current = { key: crypto.randomUUID(), body };
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
export function LearningConsole({
  base,
  data,
  detail,
}: {
  base: string;
  data?: Awaited<ReturnType<typeof learningWorkspace>>;
  detail?: Awaited<ReturnType<typeof learningAssignmentDetail>>;
}) {
  const source = data ?? detail!,
    { send, busy, error } = useCommand(base),
    a = detail?.assignment;
  return (
    <>
      {error && <p role="alert">{error}</p>}
      {data?.canWrite && (
        <form
          className="card business-form"
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
          <label>
            Title
            <input name="title" required />
          </label>
          <label>
            Description
            <textarea name="description" required minLength={3} />
          </label>
          <label>
            Provider
            <input name="provider" required />
          </label>
          <label>
            Minutes
            <input name="durationMinutes" type="number" min="1" required />
          </label>
          <label>
            Eligibility code
            <input name="requiredFor" />
          </label>
          <button className="button" disabled={busy}>
            Draft course
          </button>
        </form>
      )}
      {data?.canReview &&
        data.courses
          .filter((x) => x.status === "draft")
          .map((x) => (
            <form
              className="card business-form"
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
              <label>
                Decision
                <select name="action">
                  <option value="publish">Publish</option>
                  <option value="reject">Reject</option>
                </select>
              </label>
              <label>
                Reason
                <textarea name="reason" required minLength={3} />
              </label>
              <button className="button" disabled={busy}>
                Record review
              </button>
            </form>
          ))}
      {data?.canReview && (
        <form
          className="card business-form"
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget)),
              course = data.courses.find((x) => x.id === f.courseId);
            void send({ ...f, courseVersion: Number(course?.version) });
          }}
        >
          <h2>Assign course</h2>
          <label>
            Course
            <select name="courseId">
              {data.courses
                .filter((x) => x.status === "published")
                .map((x) => (
                  <option key={String(x.id)} value={String(x.id)}>
                    {String(x.title)}
                  </option>
                ))}
            </select>
          </label>
          <label>
            Learner
            <select name="learnerId">
              {data.people.map((x) => (
                <option key={String(x.id)} value={String(x.id)}>
                  {String(x.name)}
                </option>
              ))}
            </select>
          </label>
          <label>
            Due
            <input name="dueDate" type="date" required />
          </label>
          <label>
            Reason
            <textarea name="reason" required minLength={3} />
          </label>
          <button className="button" disabled={busy}>
            Assign course
          </button>
        </form>
      )}
      {a && a.status === "assigned" && (
        <form
          className="card business-form"
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
          <label>
            Reason
            <textarea name="reason" required minLength={3} />
          </label>
          <button className="button" disabled={busy}>
            Start
          </button>
        </form>
      )}
      {a && a.status === "in_progress" && (
        <form
          className="card business-form"
          onSubmit={(e) => {
            e.preventDefault();
            const f = Object.fromEntries(new FormData(e.currentTarget));
            void send({
              ...f,
              id: a.id,
              action: "complete",
              expectedVersion: Number(a.version),
              scoreBps: Number(f.scoreBps),
            });
          }}
        >
          <h2>Record attendance and result</h2>
          <label>
            Attendance
            <select name="attendance">
              <option value="attended">Attended</option>
              <option value="absent">Absent</option>
            </select>
          </label>
          <label>
            Score bps
            <input name="scoreBps" type="number" min="0" max="10000" required />
          </label>
          <label>
            Evidence
            <textarea name="evidence" required minLength={3} />
          </label>
          <label>
            Reason
            <textarea name="reason" required minLength={3} />
          </label>
          <button className="button" disabled={busy}>
            Complete course
          </button>
        </form>
      )}
      {a &&
        ["assigned", "in_progress"].includes(String(a.status)) &&
        source.canReview && (
          <form
            className="card business-form"
            onSubmit={(e) => {
              e.preventDefault();
              const f = Object.fromEntries(new FormData(e.currentTarget));
              void send({ ...f, id: a.id, expectedVersion: Number(a.version) });
            }}
          >
            <h2>Reviewer exception</h2>
            <label>
              Action
              <select name="action">
                <option value="waive">Waive</option>
                <option value="cancel">Cancel</option>
              </select>
            </label>
            <label>
              Reason
              <textarea name="reason" required minLength={3} />
            </label>
            <button className="button" disabled={busy}>
              Record exception
            </button>
          </form>
        )}
    </>
  );
}
