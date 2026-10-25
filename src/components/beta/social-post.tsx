"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { TextInput } from "@/components/ui/foundation";

import { PersonAvatar } from "@/components/ui/person-avatar";
import { Dialog } from "@/components/ui/dialog";
import { Alert, Button } from "@/components/ui/foundation";
import { SocialTextBody } from "./social-text-body";
import dynamic from "next/dynamic";
import {
  postContentBody,
  postContentMentions,
  postWireContent,
  postWireRichContent,
  postEditorBlocksFromStored,
  type PostEditorBlock,
} from "@/lib/social-post-editor-state";
import { workFetch } from "@/lib/work-fetch";
import { betaFetch } from "@/lib/beta-client";
import { useEffect, useRef, useState } from "react";
import Image from "next/image";
import Link from "next/link";
import { useRouter } from "next/navigation";
import {
  Heart,
  MessageCircle,
  Plus,
  Sparkles,
  Bookmark,
  Flag,
  History,
  Pencil,
  Trash2,
  ArrowUpRight,
} from "lucide-react";
import type { HomePost } from "@/server/modules/collaboration/public/social-home";
import { TimeText } from "@/components/time/time-preference-context";
import { useSocialCommand } from "./social-command";
import { SocialComments } from "./social-comments";
import { SocialAgentStatus } from "./social-agent-status";
import { HandoverAcknowledgement } from "@/components/today/handover-acknowledgement";
import { Card } from "@/components/ui/foundation";

const SocialPostContentEditor = dynamic(
  () => import("./social-post-content-editor").then((module) => module.SocialPostContentEditor),
  { ssr: false, loading: () => <InlineNotice tone="info" role="status">Loading editor…</InlineNotice> },
);

function PostImage({ src, alt }: { src: string; alt: string }) {
  const [failed, setFailed] = useState(false);
  return (
    <a href={src} target="_blank" rel="noreferrer">
      {failed ? (
        <span className="beta-image-missing">Image preview unavailable · Open {alt}</span>
      ) : (
        <Image
          unoptimized
          width={900}
          height={600}
          src={`${src}?variant=preview`}
          alt={alt}
          loading="lazy"
          onLoad={(event) => {
            if (event.currentTarget.naturalWidth <= 1 || event.currentTarget.naturalHeight <= 1)
              setFailed(true);
          }}
          onError={() => setFailed(true)}
        />
      )}
    </a>
  );
}

export function SocialPost({
  base,
  post,
  userId,
  canModerate,
  single,
  refresh,
  onDeleted,
  prepare,
}: {
  base: string;
  post: HomePost;
  userId: string;
  canModerate: boolean;
  single: boolean;
  refresh: () => Promise<void>;
  onDeleted: () => void;
  prepare: (post: HomePost, mode: "task" | "summary", useAi: boolean) => void;
}) {
  const router = useRouter();
  const postMoreRef = useRef<HTMLDetailsElement>(null);
  const api = `/api/v1${base}`,
    [commentsOpen, setCommentsOpen] = useState(single),
    [reportOpen, setReportOpen] = useState(false),
    [editing, setEditing] = useState(false),
    [editBlocks, setEditBlocks] = useState<PostEditorBlock[]>([]),
    [editResetKey, setEditResetKey] = useState(0),
    [editUploadDraftId, setEditUploadDraftId] = useState<string | null>(null),
    [editUploadedIds, setEditUploadedIds] = useState<string[]>([]),
    [deleteConfirm, setDeleteConfirm] = useState(false),
    [revisions, setRevisions] = useState<Array<{
      prior_version: number;
      action: string;
      body: string;
      created_at: number;
    }> | null>(null);
  const { run, send, busy, message, setMessage } = useSocialCommand();
  const action = (_post: HomePost, input: unknown) =>
    run(async () => {
      await send(`${api}/posts/${post.id}`, input);
      await refresh();
    });
  const pending = post.agentReplies.some((r) => ["pending", "running"].includes(r.state));
  const ownTeamPost = post.author_id === userId && post.kind === "team" && !post.linked_module;
  const canEditText = ownTeamPost;
  function beginEdit() {
    setEditBlocks(
      postEditorBlocksFromStored(
        post.content,
        post.richContent,
        post.body,
        post.mentions,
        post.content
          ? []
          : post.files.map((file) => ({ id: file.id, original_name: file.original_name })),
      ),
    );
    setEditResetKey((value) => value + 1);
    setEditUploadDraftId(null);
    setEditUploadedIds([]);
    setEditing(true);
  }
  async function saveEdit() {
    await run(async () => {
      let working = editBlocks;
      let uploadDraftId = editUploadDraftId;
      const uploadedIds = new Set(editUploadedIds);
      const pending = working.filter(
        (block): block is Extract<PostEditorBlock, { type: "image" }> =>
          block.type === "image" && !!block.file && !block.attachmentId,
      );
      if (pending.length && !uploadDraftId) {
        const draft = await send<{ id: string }>(`${api}/beta/post-edit-uploads`, {
          postId: post.id,
          expectedVersion: post.version,
        });
        uploadDraftId = draft.id;
        setEditUploadDraftId(draft.id);
      }
      for (const image of pending) {
        let uploadUrl = image.uploadUrl;
        if (!uploadUrl) {
          const intent = await send<{ uploadUrl: string }>(`${api}/uploads`, {
            ownerType: "social_draft",
            ownerId: uploadDraftId!,
            purpose: "post-image",
            filename: image.name,
            mediaType: image.file!.type,
            bytes: image.file!.size,
          });
          uploadUrl = intent.uploadUrl;
          working = working.map((block) =>
            block.type === "image" && block.slotId === image.slotId
              ? { ...block, uploadUrl }
              : block,
          );
          setEditBlocks(working);
        }
        const response = await workFetch(uploadUrl, { method: "PUT", body: image.file });
        const result = (await response.json()) as {
          data?: { id: string };
          error?: { message: string };
        };
        if (!response.ok || !result.data) {
          if (response.status === 404) {
            working = working.map((block) =>
              block.type === "image" && block.slotId === image.slotId
                ? { ...block, uploadUrl: undefined }
                : block,
            );
            setEditBlocks(working);
          }
          throw new Error(
            result.error?.message ?? "Image upload failed. Your edit is still here; try again.",
          );
        }
        uploadedIds.add(result.data.id);
        setEditUploadedIds([...uploadedIds]);
        working = working.map((block) =>
          block.type === "image" && block.slotId === image.slotId
            ? {
                ...block,
                attachmentId: result.data!.id,
                file: undefined,
                uploadUrl: undefined,
                previewUrl: undefined,
              }
            : block,
        );
        setEditBlocks(working);
      }
      const selectedUploaded = working.some(
        (block) =>
          block.type === "image" && !!block.attachmentId && uploadedIds.has(block.attachmentId),
      );
      if (uploadDraftId && !selectedUploaded) {
        await send(`${api}/beta/post-edit-uploads/${uploadDraftId}`, { action: "cancel" });
        uploadDraftId = null;
      }
      const body = postContentBody(working);
      await send(`${api}/posts/${post.id}`, {
        action: "edit",
        expectedVersion: post.version,
        body,
        mentions: postContentMentions(working),
        content: postWireContent(working),
        richContent: postWireRichContent(working),
        ...(selectedUploaded && uploadDraftId ? { editDraftId: uploadDraftId } : {}),
      });
      setEditing(false);
      setEditUploadDraftId(null);
      setEditUploadedIds([]);
      await refresh();
    });
  }
  async function cancelEdit() {
    if (editUploadDraftId)
      await run(async () => {
        await send(`${api}/beta/post-edit-uploads/${editUploadDraftId}`, { action: "cancel" });
        setEditUploadDraftId(null);
        setEditUploadedIds([]);
        setEditing(false);
      });
    else setEditing(false);
  }
  const purpose =
    post.purpose && post.purpose !== "update"
      ? post.purpose.charAt(0).toUpperCase() + post.purpose.slice(1)
      : "Update";
  useEffect(() => {
    if (!pending) return;
    const poll = () => {
      if (!document.hidden) void refresh().catch(() => {});
    };
    const timer = setInterval(poll, 3000);
    document.addEventListener("visibilitychange", poll);
    return () => {
      clearInterval(timer);
      document.removeEventListener("visibilitychange", poll);
    };
  }, [pending, refresh]);
  useEffect(() => {
    const closeOnOutsidePointer = (event: PointerEvent) => {
      const menu = postMoreRef.current;
      if (menu?.open && event.target instanceof Node && !menu.contains(event.target)) {
        menu.open = false;
      }
    };
    document.addEventListener("pointerdown", closeOnOutsidePointer, true);
    return () => document.removeEventListener("pointerdown", closeOnOutsidePointer, true);
  }, []);
  let textOffset = 0;
  const orderedContent = post.content?.map((block, index) => {
    if (block.type === "text") {
      const offset = textOffset;
      textOffset += block.text.length + 1;
      if (!block.text) return null;
      const mentions = post.mentions
        .filter((mention) => mention.start >= offset && mention.end <= offset + block.text.length)
        .map((mention) => ({
          ...mention,
          start: mention.start - offset,
          end: mention.end - offset,
        }));
      const richBlock = post.richContent?.blocks[index];
      return (
        <SocialTextBody appearance="standard"
          key={`text-${index}`}
          body={block.text}
          mentions={mentions}
          base={base}
          doc={richBlock?.type === "text" ? richBlock.doc : null}
        />
      );
    }
    const file = post.files.find((item) => item.id === block.attachmentId);
    if (!file) return null;
    return (
      <figure
        className="beta-post-inline-image"
        key={block.slotId}
        style={{ maxWidth: `${block.width ?? 100}%` }}
      >
        <PostImage
          src={`${api}/attachments/${file.id}/content`}
          alt={block.alt || file.original_name}
        />
      </figure>
    );
  });
  const hasText = post.content
    ? post.content.some((block) => block.type === "text" && !!block.text.trim())
    : !!post.body.trim();
  const photoOnly = post.files.length > 0 && !hasText;
  return (
    <Card as="article" appearance="standard" id={post.id} key={post.id} className="ui-panel beta-post">
      <header className="beta-person">
        <Link
          href={`${base}/profiles/${encodeURIComponent(post.author_id)}`}
          aria-label={`View ${post.author_name}'s profile`}
        >
          <PersonAvatar name={post.author_name} />
        </Link>
        <div>
          <strong>
            <Link href={`${base}/profiles/${encodeURIComponent(post.author_id)}`}>
              {post.author_name}
            </Link>
          </strong>
          <p>
            <Link href={`${base}/posts/${post.id}`}>
              <TimeText appearance="standard"
                value={post.created_at}
                options={{
                  day: "numeric",
                  month: "short",
                  year: "numeric",
                  hour: "2-digit",
                  minute: "2-digit",
                }}
              />
            </Link>{" "}
            · {purpose} · {post.audienceLabel}
          </p>
        </div>
        <details ref={postMoreRef} className="post-more" name="beta-social-actions">
          <summary aria-label="More post actions">•••</summary>
          <div className="post-more-menu">
            {" "}
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              className="beta-follow"
              aria-label={post.followed ? "Unfollow conversation" : "Follow conversation"}
              aria-pressed={!!post.followed}
              disabled={busy}
              onClick={() => action(post, { action: "follow", active: !post.followed })}
            >
              <Bookmark size={17} />
              {post.followed ? "Unfollow conversation" : "Follow conversation"}
            </Button>
            <Button appearance="standard"
              type="submit"
              variant="ghost"
              disabled={busy}
              onClick={() => prepare(post, "summary", false)}
            >
              <Sparkles size={16} /> Summarize
            </Button>
            {!post.hasTask && post.linkedSources.length > 0 && (
              <Button appearance="standard"
                type="submit"
                variant="ghost"
                disabled={busy}
                onClick={() => prepare(post, "task", false)}
              >
                <Plus size={16} />
                Create additional task
              </Button>
            )}
            <Button appearance="standard" type="submit" variant="ghost" onClick={() => setReportOpen((value) => !value)}>
              <Flag size={16} />
              Report{canModerate ? " / Moderate" : ""}
            </Button>
            {canEditText && (
              <Button appearance="standard" type="submit" variant="ghost" disabled={busy} onClick={beginEdit}>
                <Pencil size={16} /> Edit post
              </Button>
            )}
            {ownTeamPost && (
              <Button appearance="standard"
                type="submit"
                variant="ghost"
                disabled={busy}
                onClick={() =>
                  void run(async () => {
                    setRevisions(await betaFetch(`${api}/posts/${post.id}/revisions`));
                  })
                }
              >
                <History size={16} /> Edit history
              </Button>
            )}
            {ownTeamPost && (
              <Button appearance="standard"
                variant="ghost"
                type="button"
                className="post-delete-action"
                disabled={busy}
                onClick={() => {
                  if (postMoreRef.current) postMoreRef.current.open = false;
                  setMessage("");
                  setDeleteConfirm(true);
                }}
              >
                <Trash2 size={16} /> Delete post
              </Button>
            )}
          </div>
        </details>
      </header>
      {photoOnly && (
        <p className="beta-media-caption">
          Photo update · {post.files.length} {post.files.length === 1 ? "image" : "images"}
        </p>
      )}
      {editing ? (
        <div className="beta-post-editor" aria-label="Edit post">
          <SocialPostContentEditor
            api={api}
            blocks={editBlocks}
            onChange={setEditBlocks}
            resetKey={editResetKey}
            maxLength={30000}
            label="Edit post message"
            placeholder="Update your message…"
            disabled={busy}
          />
          <div className="beta-composer-actions">
            <Button appearance="standard"
              type="submit"
              variant="primary"
              disabled={
                busy ||
                (!postContentBody(editBlocks).trim() &&
                  !editBlocks.some((block) => block.type === "image"))
              }
              onClick={() => void saveEdit()}
            >
              Save changes
            </Button>
            <Button appearance="standard"
              type="submit"
              variant="secondary"
              disabled={busy}
              onClick={() => void cancelEdit()}
            >
              Cancel
            </Button>
          </div>
        </div>
      ) : (
        <div className="beta-post-content">
          {post.content ? (
            orderedContent
          ) : (
            <SocialTextBody appearance="standard"
              body={post.body}
              mentions={post.mentions}
              base={base}
              doc={
                post.richContent?.blocks[0]?.type === "text" ? post.richContent.blocks[0].doc : null
              }
            />
          )}
        </div>
      )}
      {post.agentReplies
        .filter((r) => r.owner_type === "post" && r.state !== "completed")
        .map((r) => (
          <SocialAgentStatus
            key={r.id}
            reply={r}
            api={api}
            userId={userId}
            onChanged={refresh}
            aiName={post.aiName}
            settingsUrl={
              canModerate
                ? `${base.match(/^\/holdings\/[^/]+/)?.[0] ?? base}/settings?section=ai`
                : undefined
            }
          />
        ))}
      {post.edited_at && !editing && <p className="small">Edited</p>}
      {revisions && (
        <section className="beta-post-history" aria-label="Edit history">
          <strong>Edit history</strong>
          {revisions.length ? (
            revisions.map((row) => (
              <div key={row.prior_version}>
                <small>
                  {row.action === "edit" ? "Before edit" : "Before deletion"} · Version{" "}
                  {row.prior_version} · <TimeText appearance="standard" value={row.created_at} />
                </small>
                <p>{row.body}</p>
              </div>
            ))
          ) : (
            <p>No earlier versions.</p>
          )}
          <Button appearance="standard" type="submit" variant="ghost" onClick={() => setRevisions(null)}>
            Close history
          </Button>
        </section>
      )}
      <Dialog appearance="standard"
        open={deleteConfirm}
        onClose={() => {
          if (busy) return;
          setDeleteConfirm(false);
          setMessage("");
          window.requestAnimationFrame(() =>
            postMoreRef.current?.querySelector<HTMLElement>("summary")?.focus(),
          );
        }}
        title="Delete post?"
      >
        <p>
          This post will disappear from the feed and its link will no longer open. Linked work
          remains available to its participants, and ERPX keeps an audit record of the deletion.
        </p>
        {message && (
          <Alert tone="danger" title="Post could not be deleted" live>
            {message}
          </Alert>
        )}
        <div className="beta-composer-actions beta-delete-actions">
          <Button appearance="standard"
            variant="secondary"
            data-autofocus
            disabled={busy}
            onClick={() => {
              setDeleteConfirm(false);
              setMessage("");
              window.requestAnimationFrame(() =>
                postMoreRef.current?.querySelector<HTMLElement>("summary")?.focus(),
              );
            }}
          >
            Keep post
          </Button>
          <Button appearance="standard"
            variant="danger"
            pending={busy}
            onClick={() =>
              void run(async () => {
                await send(`${api}/posts/${post.id}`, { action: "retract" });
                setDeleteConfirm(false);
                onDeleted();
                if (single) router.replace(`${base}/home`);
              })
            }
          >
            {busy ? "Deleting…" : "Delete post"}
          </Button>
        </div>
      </Dialog>
      {!post.content && (
        <div className="beta-images">
          {post.files.map((f) => (
            <PostImage
              key={f.id}
              src={`${api}/attachments/${f.id}/content`}
              alt={f.original_name}
            />
          ))}
        </div>
      )}
      {post.task ? (
        <Link className="beta-task-link" href={`${base}/staff-tasks/${post.task.id}`}>
          Open linked work · Task · {post.task.state.replaceAll("_", " ")}{" "}
          <ArrowUpRight size={15} />
        </Link>
      ) : post.trackingState ? (
        <p className="beta-task-link">Work progress · {post.trackingState.replaceAll("_", " ")}</p>
      ) : post.hasTask ? (
        <p className="small">A task is linked. Its details are available to its participants.</p>
      ) : null}
      {post.linkedSources.length > 0 && (
        <div className="beta-post-sources" aria-label="Linked work">
          {post.linkedSources.map((source, index) =>
            source.available ? (
              <Link
                className="beta-task-link"
                key={`${source.kind}:${source.id}`}
                href={`${base}${source.href}`}
              >
                Open linked work · {source.label} · {source.title} <ArrowUpRight size={15} />
              </Link>
            ) : (
              <span className="small" key={`unavailable-${index}`}>
                Linked work unavailable to your role
              </span>
            ),
          )}
        </div>
      )}
      {post.purpose === "handover" &&
        post.audience_kind === "participants" &&
        post.linkedSources.length > 0 && (
          <HandoverAcknowledgement appearance="standard" api={api} postId={post.id} version={post.version} />
        )}
      {(post.appreciationCount > 0 || post.commentCount > 0) && (
        <div className="beta-post-counts">
          {post.appreciationCount > 0 && <span>{post.appreciationCount} Thanks</span>}
          {post.commentCount > 0 && <span>{post.commentCount} comments</span>}
        </div>
      )}
      <div className="beta-post-actions">
        <Button appearance="standard"
          type="button"
          variant="ghost"
          disabled={busy}
          aria-pressed={!!post.appreciated}
          aria-label="Thanks"
          title="Thanks"
          onClick={() =>
            action(post, {
              action: "appreciate",
              active: !post.appreciated,
            })
          }
        >
          <Heart size={16} aria-hidden="true" />
        </Button>
        <Button appearance="standard"
          type="button"
          variant="ghost"
          aria-expanded={commentsOpen}
          aria-label="Comment"
          title="Comment"
          onClick={() => setCommentsOpen((value) => !value)}
        >
          <MessageCircle size={16} aria-hidden="true" />
        </Button>

        {!post.hasTask && !post.linkedSources.length && (
          <Button appearance="standard"
            type="button"
            variant="ghost"
            disabled={busy}
            onClick={() => prepare(post, "task", false)}
            aria-label="Create task"
            title="Create task"
          >
            <Plus size={16} aria-hidden="true" />
          </Button>
        )}
      </div>
      <div>
        <SocialComments
          expanded={commentsOpen}
          onExpand={() => setCommentsOpen(true)}
          api={api}
          post={post}
          userId={userId}
          onChanged={refresh}
          settingsUrl={
            canModerate
              ? `${base.match(/^\/holdings\/[^/]+/)?.[0] ?? base}/settings?section=ai`
              : undefined
          }
        />
      </div>
      <details className="beta-report" open={reportOpen} hidden={!reportOpen}>
        <summary>Report{canModerate ? " / Moderate" : ""}</summary>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            action(post, {
              action: "report",
              body: new FormData(e.currentTarget).get("reason"),
            });
          }}
        >
          <TextInput appearance="standard"
            name="reason"
            placeholder="Reason for reporting"
            aria-label="Reason for reporting"
            required
            maxLength={2000}
          />
          <Button appearance="standard" clientManaged type="submit" variant="ghost" disabled={busy}>
            Send report
          </Button>
        </form>
        {canModerate && (
          <Button appearance="standard"
            type="submit"
            variant="ghost"
            disabled={busy}
            onClick={() => action(post, { action: "hide" })}
          >
            Hide this post
          </Button>
        )}
      </details>
      {!deleteConfirm && <InlineNotice tone="info" role="status">{message}</InlineNotice>}
    </Card>
  );
}
