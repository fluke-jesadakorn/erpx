"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { LockKeyhole, Sparkles } from "lucide-react";
import type { HomePost } from "@/server/modules/collaboration/public/social-home";
import type { AiResult } from "@/server/modules/collaboration/public/social-ai";
import type { TaskSource } from "@/server/modules/collaboration/public/social-task";
import { betaFetch } from "@/lib/shared/beta-client";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/foundation";
import { useSocialCommand } from "@/components/collaboration/social-command";
import { localMinuteAt } from "@/lib/work_projection/calendar-timeline";
import { PreferredDateTimeField } from "@/components/people_payroll/time/preferred-date-time-field";
import { useDisplayTimeZone } from "@/components/people_payroll/time/time-preference-context";
import type { Meta } from "@/components/collaboration/social-types";
import styles from "@/components/collaboration/social-task-dialog.module.css";
export function SocialTaskDialog({
  base,
  selected,
  meta,
  userId,
  mode,
  open,
  onClose,
}: {
  base: string;
  selected: HomePost;
  meta: Meta;
  userId: string;
  mode: "task" | "summary";
  open: boolean;
  onClose: () => void;
}) {
  const router = useRouter(),
    api = `/api/v1${base}`;
  const { timeZone } = useDisplayTimeZone();
  const { run, send, busy, message, setMessage } = useSocialCommand();
  const [source, setSource] = useState<TaskSource | null>(null),
    [ai, setAi] = useState<AiResult | null>(null);
  const [title, setTitle] = useState(""),
    [description, setDescription] = useState(selected.body),
    [assignee, setAssignee] = useState(""),
    [reviewer, setReviewer] = useState(""),
    [due, setDue] = useState("");
  const [commentIds, setCommentIds] = useState<string[]>([]),
    [aiPreparing, setAiPreparing] = useState(false);
  const sourceQuery = new URLSearchParams(commentIds.map((id) => ["commentId", id])).toString();
  useEffect(() => {
    let active = true;
    betaFetch<{
      post: { version: number };
      hash: string;
      commentIds: string[];
    }>(`${api}/posts/${selected.id}/task-source?${sourceQuery}`)
      .then((snapshot) => {
        if (active)
          setSource({
            postId: selected.id,
            sourceVersion: snapshot.post.version,
            sourceHash: snapshot.hash,
            commentIds: snapshot.commentIds,
            aiDraftId: null,
          });
      })
      .catch((e) => {
        if (active)
          setMessage(e instanceof Error ? e.message : "Unable to read conversation context.");
      });
    return () => {
      active = false;
    };
  }, [api, selected.id, sourceQuery, setMessage]);
  async function prepare(post: HomePost, kind: "task" | "summary", useAi: boolean) {
    setAi(null);
    setSource(null);
    if (useAi) setAiPreparing(true);
    try {
      await run(async () => {
        const snapshot = await betaFetch<{
          post: { version: number };
          hash: string;
          commentIds: string[];
        }>(
          `${api}/posts/${post.id}/task-source?${commentIds.map((id) => `commentId=${encodeURIComponent(id)}`).join("&")}`,
        );
        setSource({
          postId: post.id,
          sourceVersion: snapshot.post.version,
          sourceHash: snapshot.hash,
          commentIds,
          aiDraftId: null,
        });
        if (useAi) {
          const generated = await send<{
            id: string;
            result: AiResult;
            sourceVersion: number;
            sourceHash: string;
          }>(`${api}/posts/${post.id}/ai-drafts`, { kind, commentIds });
          setAi(generated.result);
          setSource({
            postId: post.id,
            sourceVersion: generated.sourceVersion,
            sourceHash: generated.sourceHash,
            commentIds,
            aiDraftId: generated.id,
          });
          setTitle((value) => value || generated.result.title || "");
          setDescription((value) =>
            value && value !== post.body ? value : (generated.result.description ?? post.body),
          );
          setAssignee((value) => value || generated.result.assigneeUserId || "");
          setReviewer((value) => value || generated.result.reviewerUserId || "");
          if (generated.result.dueAt) {
            const date = new Date(generated.result.dueAt);
            const local = localMinuteAt(date.getTime(), timeZone);
            setDue((value) => value || `${local.date}T${local.time}`);
          }
        }
      });
    } finally {
      if (useAi) setAiPreparing(false);
    }
  }
  return (
    <Dialog appearance="standard"
      open={open}
      title={
        mode === "summary" ? "Private conversation summary" : "Create work from this conversation"
      }
      onClose={onClose}
      className={styles.dialog}
      bodyClassName={styles.body}
      dismissDisabled={busy}
     presentation="form-drawer">
      <div className={styles.draft}>
        <div className={styles.intro}>
          <span className={styles.privacy}><LockKeyhole size={14} aria-hidden="true" /> Private draft</span>
          <p>Review the conversation and task details before saving.</p>
        </div>
        <details className={styles.source}>
          <summary>Conversation source · {commentIds.length} comments included</summary>
          <div className={styles.sourceContent}>
            <p className={styles.sourcePreview}>{selected.body || "This conversation contains media."}</p>
            <div className={styles.sourceActions}>
              <Link href={`${base}/posts/${selected.id}`} target="_blank" rel="noopener noreferrer">Open source conversation ↗</Link>
              <Button appearance="standard" variant="ghost" disabled={busy} onClick={() => prepare(selected, "task", false)}>Refresh source</Button>
            </div>
        <details className={styles.comments}>
          <summary>Comments to include ({commentIds.length})</summary>
          {selected.comments.length === 0 && <p>No comments available to include.</p>}
          {selected.comments.map((comment) => (
            <Field appearance="standard" orientation="inline" key={comment.id}>
              <TextInput appearance="standard"
                type="checkbox"
                disabled={busy}
                checked={commentIds.includes(comment.id)}
                onChange={(e) => {
                  setCommentIds((ids) =>
                    e.target.checked ? [...ids, comment.id] : ids.filter((id) => id !== comment.id),
                  );
                  setSource(null);
                  setAi(null);
                }}
              />
              {comment.author_name}: {comment.body}
            </Field>
          ))}
        </details>
          </div>
        </details>
        {ai && (
          <section className={styles.aiResult}>
            <strong>AI draft</strong>
            <p>{ai.summary}</p>
            {ai.missing.length > 0 && <p>Needs your input: {ai.missing.join(" · ")}</p>}
            <div>
              {ai.citations.map((id) => (
                <Link key={id} href={`${base}/posts/${selected.id}#${id}`}>
                  Source {id === selected.id ? "post" : "comment"} ↗{" "}
                </Link>
              ))}
            </div>
          </section>
        )}
        {mode === "summary" ? (
          <>
            <Button appearance="standard"
              variant="secondary"
              className="button secondary"
              disabled={busy}
              pending={aiPreparing}
              onClick={() => prepare(selected, "summary", true)}
            >
              {aiPreparing ? "Preparing summary…" : "Prepare summary"}
            </Button>
            <Button appearance="standard" type="submit" variant="ghost" onClick={onClose}>
              Back to conversation
            </Button>
          </>
        ) : (
          <>
            <div className={styles.aiTools}>
              <div><strong>Start with a little help</strong><p>AI can suggest details. You can edit every field.</p></div>
              <Button appearance="standard" variant="secondary" disabled={busy} pending={aiPreparing}
                onClick={() => prepare(selected, "task", true)}>
                {aiPreparing ? "Preparing task…" : <><Sparkles size={15} aria-hidden="true" /> Let AI prepare this task</>}
              </Button>
            </div>
            <form
              className={styles.form}
              onSubmit={(e) => {
                e.preventDefault();
                const dueAt = String(new FormData(e.currentTarget).get("dueAt"));
                run(async () => {
                  if (!source) throw new Error("Reload the conversation before creating work.");
                  const result = await send<{ id: string }>(`${api}/posts/${selected.id}/task`, {
                    source,
                    task: {
                      title,
                      description,
                      assigneeUserId: assignee,
                      dueAt,
                      projectId: null,
                      startApprovers: [],
                      completionApprovers: [reviewer],
                    },
                  });
                  router.push(`${base}/staff-tasks/${result.id}`);
                });
              }}
            >
              <Field appearance="standard" label="Task title" required>
                <TextInput appearance="standard"
                  placeholder="Give this task a clear, actionable title"
                  data-autofocus
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  required
                  minLength={2}
                  maxLength={200}
                />
              </Field>
              <Field appearance="standard" label="What needs to be done?">
                <TextArea appearance="standard"
                  rows={3}
                  placeholder="Describe the expected result and any useful details"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  maxLength={10000}
                />
              </Field>
              <div className={styles.assignment}>
              <Field appearance="standard" label="Responsible teammate" required>
                <Select appearance="standard"
                  aria-label="Responsible teammate"
                  required
                  value={assignee}
                  onChange={(e) => {
                    setAssignee(e.target.value);
                    if (reviewer === e.target.value) setReviewer("");
                  }}
                >
                  <option value="">Choose a person</option>
                  {meta.people.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </Select>
              </Field>
              <PreferredDateTimeField appearance="standard"
                name="dueAt"
                label="Due date"
                required
                valueLocal={due}
                onLocalChange={setDue}
              />
              </div>
              <Field appearance="standard" label="Independent completion reviewer" required hint="Choose someone other than yourself or the responsible teammate.">
                <Select appearance="standard"
                  aria-label="Independent completion reviewer"
                  required
                  value={reviewer}
                  onChange={(e) => setReviewer(e.target.value)}
                >
                  <option value="">Choose a reviewer</option>
                  {meta.people
                    .filter((p) => p.id !== userId && p.id !== assignee)
                    .map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.name}
                      </option>
                    ))}
                </Select>
              </Field>
              {message && <InlineNotice tone="danger" className={styles.error} role="alert">{message}</InlineNotice>}
              {!source && !busy && !message && <InlineNotice tone="info" className={styles.loading} role="status">Loading conversation source…</InlineNotice>}
              <footer className={styles.footer}>
                <p>Save as a draft, then submit for work approval.</p>
                <div className={styles.footerActions}>
                  <Button appearance="standard" variant="ghost" disabled={busy} onClick={onClose}>Cancel</Button>
                  <Button appearance="standard" clientManaged type="submit" variant="primary" disabled={busy || !source} pending={busy && !aiPreparing}>
                    {busy && !aiPreparing ? "Saving draft…" : "Create draft task"}
                  </Button>
                </div>
              </footer>
            </form>
          </>
        )}
        {mode === "summary" && message && <InlineNotice tone="danger" className={styles.error} role="alert">{message}</InlineNotice>}
      </div>
    </Dialog>
  );
}
