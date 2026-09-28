"use client";
import { useState, useEffect } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { Sparkles } from "lucide-react";
import type { HomePost } from "@/server/social-home";
import type { AiResult } from "@/server/social-ai";
import type { TaskSource } from "@/server/social-task";
import { betaFetch } from "@/lib/beta-client";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/foundation";
import { useSocialCommand } from "./social-command";
import type { Meta } from "./social-types";
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
  const sourceQuery = new URLSearchParams(
    commentIds.map((id) => ["commentId", id]),
  ).toString();
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
          setMessage(
            e instanceof Error
              ? e.message
              : "Unable to read conversation context.",
          );
      });
    return () => {
      active = false;
    };
  }, [api, selected.id, sourceQuery, setMessage]);
  async function prepare(
    post: HomePost,
    kind: "task" | "summary",
    useAi: boolean,
  ) {
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
            value && value !== post.body
              ? value
              : (generated.result.description ?? post.body),
          );
          setAssignee(
            (value) => value || generated.result.assigneeUserId || "",
          );
          setReviewer(
            (value) => value || generated.result.reviewerUserId || "",
          );
          if (generated.result.dueAt) {
            const date = new Date(generated.result.dueAt);
            setDue(
              (value) =>
                value ||
                new Date(date.getTime() - date.getTimezoneOffset() * 60000)
                  .toISOString()
                  .slice(0, 16),
            );
          }
        }
      });
    } finally {
      if (useAi) setAiPreparing(false);
    }
  }
  return (
    <Dialog
      open={open}
      title={
        mode === "summary"
          ? "Private conversation summary"
          : "Create work from this conversation"
      }
      onClose={onClose}
    >
      <div className="beta-task-draft">
        <p>
          Only you can see this draft. Review the source and every field before
          saving.
        </p>
        <Link href={`${base}/posts/${selected.id}`}>
          Open source conversation ↗
        </Link>
        <details className="source-comments">
          <summary>Comments to include ({commentIds.length})</summary>
          {selected.comments.map((comment) => (
            <label key={comment.id}>
              <input
                type="checkbox"
                checked={commentIds.includes(comment.id)}
                onChange={(e) => {
                  setCommentIds((ids) =>
                    e.target.checked
                      ? [...ids, comment.id]
                      : ids.filter((id) => id !== comment.id),
                  );
                  setSource(null);
                  setAi(null);
                }}
              />
              {comment.author_name}: {comment.body}
            </label>
          ))}
        </details>
        {ai && (
          <section className="beta-ai-result">
            <strong>AI draft</strong>
            <p>{ai.summary}</p>
            {ai.missing.length > 0 && (
              <p>Needs your input: {ai.missing.join(" · ")}</p>
            )}
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
            <Button
              variant="secondary"
              className="button secondary"
              disabled={busy}
              pending={aiPreparing}
              onClick={() => prepare(selected, "summary", true)}
            >
              {aiPreparing ? "Preparing summary…" : "Prepare summary"}
            </Button>
            <button className="button ghost" onClick={onClose}>
              Back to conversation
            </button>
          </>
        ) : (
          <>
            <Button
              variant="secondary"
              className="button secondary"
              disabled={busy}
              pending={aiPreparing}
              onClick={() => prepare(selected, "task", true)}
            >
              {aiPreparing ? (
                "Preparing task…"
              ) : (
                <>
                  <Sparkles size={15} /> Let AI prepare this task
                </>
              )}
            </Button>
            <details>
              <summary>Conversation context</summary>
              <button
                className="button secondary"
                disabled={busy}
                onClick={() => prepare(selected, "task", false)}
              >
                Refresh source
              </button>
            </details>
            <form
              className="beta-form"
              onSubmit={(e) => {
                e.preventDefault();
                run(async () => {
                  if (!source)
                    throw new Error(
                      "Reload the conversation before creating work.",
                    );
                  const result = await send<{ id: string }>(
                    `${api}/posts/${selected.id}/task`,
                    {
                      source,
                      task: {
                        title,
                        description,
                        assigneeUserId: assignee,
                        dueAt: new Date(due).toISOString(),
                        projectId: null,
                        startApprovers: [],
                        completionApprovers: [reviewer],
                      },
                    },
                  );
                  router.push(`${base}/staff-tasks/${result.id}`);
                });
              }}
            >
              <label>
                Task title
                <input
                  value={title}
                  onChange={(e) => setTitle(e.target.value)}
                  required
                  minLength={2}
                  maxLength={200}
                />
              </label>
              <label>
                What needs to be done?
                <textarea
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  maxLength={10000}
                />
              </label>
              <label>
                Responsible teammate
                <select
                  aria-label="Responsible teammate"
                  required
                  value={assignee}
                  onChange={(e) => setAssignee(e.target.value)}
                >
                  <option value="">Choose a person</option>
                  {meta.people.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                Due date · your local time
                <input
                  type="datetime-local"
                  required
                  value={due}
                  onChange={(e) => setDue(e.target.value)}
                />
              </label>
              <label>
                Independent completion reviewer
                <select
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
                </select>
              </label>
              <p>
                This creates a draft task. Submit it through the existing work
                approval process.
              </p>
              <button className="button" disabled={busy || !source}>
                Confirm and create draft task
              </button>
            </form>
          </>
        )}
        <p role="status">{busy ? "Preparing…" : message}</p>
      </div>
    </Dialog>
  );
}
