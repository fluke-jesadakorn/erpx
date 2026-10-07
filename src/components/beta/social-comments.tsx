"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextInput } from "@/components/ui/foundation";

import { workFetch } from "@/lib/work-fetch";
import { useRef, useState } from "react";
import type { Mention } from "@/server/modules/collaboration/public/rules/social-text";
import type { JSONContent } from "@tiptap/core";
import { plainRichContent } from "@/lib/home-rich-content";
import { SocialTextInput } from "./social-text-input";
import { SocialTextBody } from "./social-text-body";
import { SocialAgentStatus } from "./social-agent-status";
import type { HomePost } from "@/server/modules/collaboration/public/social-home";
import { useSocialCommand } from "./social-command";
import { ImagePlus, Link2, MoreHorizontal, Pencil, Send, Trash2, X } from "lucide-react";
import Link from "next/link";

type CommentImage = {
  slotId: string;
  attachmentId: string | null;
  name: string;
  alt: string;
  width?: number;
  file?: File;
  uploadUrl?: string;
};
type ReplyTarget = { id: string; name: string };

export function SocialComments({
  api,
  post,
  userId,
  onChanged,
  settingsUrl,
}: {
  api: string;
  post: HomePost;
  userId: string;
  onChanged: () => Promise<void>;
  settingsUrl?: string;
}) {
  const [loaded, setLoaded] = useState<HomePost["comments"]>([]);
  const [deletedIds, setDeletedIds] = useState<string[]>([]);
  const [edited, setEdited] = useState<
    Record<
      string,
      {
        body: string;
        mentions: Mention[];
        richContent: HomePost["comments"][number]["richContent"];
      }
    >
  >({});
  const [cursor, setCursor] = useState<string | null | undefined>(undefined);
  const comments = [
    ...new Map(
      [...loaded, ...post.comments].map((comment) => [
        comment.id,
        { ...comment, ...edited[comment.id] },
      ]),
    ).values(),
  ]
    .filter((comment) => !deletedIds.includes(comment.id))
    .sort((a, b) => a.created_at - b.created_at || a.id.localeCompare(b.id));
  const byId = new Map(comments.map((comment) => [comment.id, comment]));
  const roots = comments.filter(
    (comment) => !comment.parent_comment_id || !byId.has(comment.parent_comment_id),
  );
  const children = new Map<string, typeof comments>();
  for (const comment of comments)
    if (comment.parent_comment_id) {
      children.set(comment.parent_comment_id, [
        ...(children.get(comment.parent_comment_id) ?? []),
        comment,
      ]);
    }
  const ordered: typeof comments = [];
  const appendThread = (comment: (typeof comments)[number], visited = new Set<string>()) => {
    if (visited.has(comment.id)) return;
    ordered.push(comment);
    const nextVisited = new Set(visited).add(comment.id);
    for (const child of children.get(comment.id) ?? []) appendThread(child, nextVisited);
  };
  for (const root of roots) appendThread(root);

  const [body, setBody] = useState("");
  const [mentions, setMentions] = useState<Mention[]>([]);
  const [richDoc, setRichDoc] = useState<JSONContent | null>(null);
  const [images, setImages] = useState<CommentImage[]>([]);
  const [commentDraftId, setCommentDraftId] = useState<string | null>(null);
  const [replyTo, setReplyTo] = useState<ReplyTarget | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editBody, setEditBody] = useState("");
  const [editMentions, setEditMentions] = useState<Mention[]>([]);
  const [editRichDoc, setEditRichDoc] = useState<JSONContent | null>(null);
  const replyComposerRef = useRef<HTMLFormElement>(null);
  const { run, send, busy, message } = useSocialCommand();

  const loadOlder = () =>
    run(async () => {
      const response = await workFetch(
        `${api}/posts/${post.id}/comments${cursor ? `?cursor=${encodeURIComponent(cursor)}` : ""}`,
        { cache: "no-store" },
      );
      const result = await response.json();
      if (!response.ok) throw new Error(result.error?.message ?? "Unable to load comments");
      setLoaded((old) => [...old, ...result.data.items]);
      setCursor(result.data.nextCursor);
    });
  const focusReplyComposer = (target: ReplyTarget) => {
    setReplyTo(target);
    requestAnimationFrame(() => {
      const form = replyComposerRef.current;
      if (!form) return;
      form.scrollIntoView({
        behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
        block: "center",
      });
      form
        .querySelector<HTMLElement>("textarea, [contenteditable='true']")
        ?.focus({ preventScroll: true });
    });
  };
  const deleteComment = (commentId: string) => {
    if (!window.confirm("Delete this comment?")) return;
    void run(async () => {
      await send(`${api}/posts/${post.id}`, { action: "delete_comment", commentId });
      setDeletedIds((ids) => [...ids, commentId]);
      if (replyTo?.id === commentId) setReplyTo(null);
      await onChanged();
    });
  };
  const startEdit = (comment: (typeof comments)[number]) => {
    setEditingId(comment.id);
    setEditBody(comment.body);
    setEditMentions(comment.mentions);
    setEditRichDoc(
      comment.richContent?.blocks[0]?.type === "text" ? comment.richContent.blocks[0].doc : null,
    );
  };
  const saveEdit = (comment: (typeof comments)[number]) =>
    void run(async () => {
      const doc =
        editRichDoc ??
        (plainRichContent(editBody, editMentions).blocks[0] as { doc: JSONContent }).doc;
      const richContent = {
        version: 1 as const,
        blocks: [
          { type: "text" as const, doc },
          ...(comment.richContent?.blocks.filter((block) => block.type === "image") ?? []),
        ],
      } as HomePost["comments"][number]["richContent"];
      await send(`${api}/posts/${post.id}`, {
        action: "edit_comment",
        commentId: comment.id,
        body: editBody,
        mentions: editMentions,
        richContent,
      });
      setEdited((current) => ({
        ...current,
        [comment.id]: { body: editBody, mentions: editMentions, richContent },
      }));
      setEditingId(null);
      await onChanged();
    });

  function composer() {
    return (
      <form
        ref={replyComposerRef}
        className="beta-comment-form"
        key={replyTo?.id ?? "new-comment"}
        aria-label={replyTo ? `Reply to ${replyTo.name}` : "Write a comment"}
        onSubmit={(event) => {
          event.preventDefault();
          run(async () => {
            let draftId = commentDraftId;
            let working = images;
            if (working.length && !draftId) {
              const draft = await send<{ id: string }>(`${api}/beta/comment-uploads`, {
                postId: post.id,
              });
              draftId = draft.id;
              setCommentDraftId(draft.id);
            }
            for (const image of working.filter((item) => !!item.file && !item.attachmentId)) {
              let uploadUrl = image.uploadUrl;
              if (!uploadUrl) {
                const intent = await send<{ uploadUrl: string }>(`${api}/uploads`, {
                  ownerType: "social_draft",
                  ownerId: draftId,
                  purpose: "post-image",
                  filename: image.name,
                  mediaType: image.file!.type,
                  bytes: image.file!.size,
                });
                uploadUrl = intent.uploadUrl;
                working = working.map((item) =>
                  item.slotId === image.slotId ? { ...item, uploadUrl } : item,
                );
                setImages(working);
              }
              const response = await workFetch(uploadUrl!, { method: "PUT", body: image.file });
              const result = (await response.json()) as {
                data?: { id: string };
                error?: { message: string };
              };
              if (!response.ok || !result.data) {
                if (response.status === 404) {
                  working = working.map((item) =>
                    item.slotId === image.slotId ? { ...item, uploadUrl: undefined } : item,
                  );
                  setImages(working);
                }
                throw Error(result.error?.message ?? "Image upload failed. Try again.");
              }
              working = working.map((item) =>
                item.slotId === image.slotId
                  ? {
                      ...item,
                      attachmentId: result.data!.id,
                      file: undefined,
                      uploadUrl: undefined,
                    }
                  : item,
              );
              setImages(working);
            }
            const textDoc =
              richDoc ?? (plainRichContent(body, mentions).blocks[0] as { doc: JSONContent }).doc;
            await send(`${api}/posts/${post.id}`, {
              action: "comment",
              body,
              mentions,
              richContent: {
                version: 1,
                blocks: [
                  { type: "text", doc: textDoc },
                  ...working.map((item) => ({
                    type: "image" as const,
                    slotId: item.slotId,
                    attachmentId: item.attachmentId,
                    name: item.name,
                    alt: item.alt,
                    width: item.width,
                  })),
                ],
              },
              commentDraftId: draftId ?? undefined,
              parentCommentId: replyTo?.id,
            });
            setBody("");
            setMentions([]);
            setRichDoc(null);
            setImages([]);
            setCommentDraftId(null);
            setReplyTo(null);
            await onChanged();
          });
        }}
      >
        {replyTo && (
          <InlineNotice tone="info" className="beta-comment-replying" role="status" aria-live="polite">
            <span>Replying to {replyTo.name}</span>
            <Button appearance="standard"
              variant="ghost"
              type="button"
              onClick={() => setReplyTo(null)}
              aria-label="Cancel reply"
            >
              <X size={16} />
            </Button>
          </InlineNotice>
        )}
        <div className="beta-comment-editor">
          <SocialTextInput appearance="standard"
            api={api}
            value={body}
            mentions={mentions}
            richDoc={richDoc}
            onChange={(nextBody, nextMentions) => {
              setBody(nextBody);
              setMentions(nextMentions);
              setRichDoc(null);
            }}
            onRichChange={(nextBody, nextMentions, doc) => {
              setBody(nextBody);
              setMentions(nextMentions);
              setRichDoc(doc);
            }}
            onPasteImages={(files) =>
              setImages((current) => [
                ...current,
                ...files
                  .filter(
                    (file) =>
                      ["image/png", "image/jpeg", "image/webp"].includes(file.type) &&
                      file.size <= 10 * 1024 * 1024,
                  )
                  .slice(0, 10 - current.length)
                  .map((file) => ({
                    slotId: crypto.randomUUID(),
                    attachmentId: null,
                    name: file.name || "image",
                    alt: file.name || "Image",
                    file,
                  })),
              ])
            }
            maxLength={10000}
            label={replyTo ? `Reply to ${replyTo.name}` : "Write a comment"}
            placeholder={replyTo ? `Reply to ${replyTo.name}…` : "Add to the conversation…"}
            disabled={busy}
          />
        </div>
        {images.length > 0 && (
          <div className="beta-comment-attachment-list">
            {images.map((image) => (
              <div className="beta-comment-attachment" key={image.slotId}>
                <span>
                  {image.name}
                  {image.attachmentId ? " · uploaded" : ""}
                </span>
                <TextInput appearance="standard"
                  aria-label={`Description for ${image.name}`}
                  value={image.alt}
                  maxLength={500}
                  placeholder="Image description"
                  onChange={(event) =>
                    setImages((current) =>
                      current.map((item) =>
                        item.slotId === image.slotId ? { ...item, alt: event.target.value } : item,
                      ),
                    )
                  }
                />
                <Field appearance="standard">
                  Width{" "}
                  <Select appearance="standard"
                    aria-label={`Width for ${image.name}`}
                    value={image.width ?? 100}
                    onChange={(event) =>
                      setImages((current) =>
                        current.map((item) =>
                          item.slotId === image.slotId
                            ? { ...item, width: Number(event.target.value) }
                            : item,
                        ),
                      )
                    }
                  >
                    {[25, 50, 75, 100].map((width) => (
                      <option key={width} value={width}>
                        {width}%
                      </option>
                    ))}
                  </Select>
                </Field>
                <Button appearance="standard"
                  variant="ghost"
                  type="button"
                  onClick={() =>
                    setImages((current) => current.filter((item) => item.slotId !== image.slotId))
                  }
                  aria-label={`Remove ${image.name}`}
                >
                  <X size={16} />
                </Button>
              </div>
            ))}
          </div>
        )}
        <div className="beta-comment-form-actions">
          <Field appearance="standard" className="beta-comment-upload">
            <ImagePlus size={16} aria-hidden="true" /> Add images
            <TextInput appearance="standard"
              type="file"
              aria-label="Add images to comment"
              accept="image/png,image/jpeg,image/webp"
              multiple
              onChange={(event) => {
                const files = Array.from(event.target.files ?? []);
                setImages((current) => [
                  ...current,
                  ...files
                    .filter(
                      (file) =>
                        ["image/png", "image/jpeg", "image/webp"].includes(file.type) &&
                        file.size <= 10 * 1024 * 1024,
                    )
                    .slice(0, 10 - current.length)
                    .map((file) => ({
                      slotId: crypto.randomUUID(),
                      attachmentId: null,
                      name: file.name,
                      alt: file.name,
                      file,
                    })),
                ]);
                event.target.value = "";
              }}
            />
          </Field>
          {images.length > 0 && (
            <Button appearance="standard"
              variant="ghost"
              type="button"
              className="beta-comment-clear"
              disabled={busy}
              onClick={() =>
                void run(async () => {
                  if (commentDraftId)
                    await send(`${api}/beta/comment-uploads/${commentDraftId}`, {
                      action: "cancel",
                    });
                  setImages([]);
                  setCommentDraftId(null);
                })
              }
            >
              Clear images
            </Button>
          )}
          <Button appearance="standard"
            clientManaged variant="ghost"
            type="submit"
            className="beta-comment-submit"
            disabled={busy || (!body.trim() && !images.length)}
          >
            <Send size={16} aria-hidden="true" /> {replyTo ? "Reply" : "Comment"}
          </Button>
        </div>
      </form>
    );
  }

  return (
    <>
      <div className="beta-comments">
        {ordered.map((comment) => (
          <div
            id={comment.id}
            className={`beta-comment${comment.parent_comment_id ? " beta-comment-reply" : ""}`}
            key={comment.id}
          >
            <div>
              <details className="beta-comment-more" name="beta-social-actions">
                <summary aria-label={`More actions for comment by ${comment.author_name}`}>
                  <MoreHorizontal size={18} aria-hidden="true" />
                </summary>
                <div className="beta-comment-menu">
                  <Button appearance="standard"
                    variant="ghost"
                    type="button"
                    onClick={(event) => {
                      event.currentTarget.closest("details")!.open = false;
                      void navigator.clipboard.writeText(
                        `${window.location.origin}${window.location.pathname}#${comment.id}`,
                      );
                    }}
                  >
                    <Link2 size={16} aria-hidden="true" /> Copy link
                  </Button>
                  {comment.author_kind !== "agent" && comment.author_id === userId && (
                    <Button appearance="standard"
                      variant="ghost"
                      type="button"
                      disabled={busy}
                      onClick={(event) => {
                        event.currentTarget.closest("details")!.open = false;
                        startEdit(comment);
                      }}
                    >
                      <Pencil size={16} aria-hidden="true" /> Edit
                    </Button>
                  )}
                  {(comment.author_id === userId ||
                    post.agentReplies.some(
                      (reply) => reply.reply_id === comment.id && reply.actor_id === userId,
                    )) && (
                    <Button appearance="standard"
                      variant="ghost"
                      type="button"
                      disabled={busy}
                      onClick={(event) => {
                        event.currentTarget.closest("details")!.open = false;
                        deleteComment(comment.id);
                      }}
                    >
                      <Trash2 size={16} aria-hidden="true" /> Delete
                    </Button>
                  )}
                </div>
              </details>
              <strong>
                {comment.author_kind === "agent" ? (
                  comment.author_name
                ) : (
                  <Link
                    href={`${api.replace(/^\/api\/v1/, "").replace(/\/beta$/, "")}/profiles/${encodeURIComponent(comment.author_id)}`}
                  >
                    {comment.author_name}
                  </Link>
                )}
                {comment.author_kind === "agent" && comment.author_name !== "AI" ? " · AI" : ""}
              </strong>
              {comment.author_kind === "agent" &&
                post.agentReplies.find((reply) => reply.reply_id === comment.id)
                  ?.requester_name && (
                  <small>
                    AI reply requested by{" "}
                    {
                      post.agentReplies.find((reply) => reply.reply_id === comment.id)
                        ?.requester_name
                    }
                  </small>
                )}
              {comment.parent_comment_id && (
                <small>Reply to {comment.parent_author_name ?? "an earlier comment"}</small>
              )}
              {editingId === comment.id ? (
                <div className="beta-comment-edit">
                  <SocialTextInput appearance="standard"
                    api={api}
                    value={editBody}
                    mentions={editMentions}
                    richDoc={editRichDoc}
                    onChange={(nextBody, nextMentions) => {
                      setEditBody(nextBody);
                      setEditMentions(nextMentions);
                      setEditRichDoc(null);
                    }}
                    onRichChange={(nextBody, nextMentions, doc) => {
                      setEditBody(nextBody);
                      setEditMentions(nextMentions);
                      setEditRichDoc(doc);
                    }}
                    maxLength={10000}
                    label="Edit comment"
                    placeholder="Edit your comment…"
                    disabled={busy}
                  />
                  <div className="beta-comment-edit-actions">
                    <Button appearance="standard"
                      variant="ghost"
                      type="button"
                      disabled={busy}
                      onClick={() => setEditingId(null)}
                    >
                      Cancel
                    </Button>
                    <Button appearance="standard"
                      variant="ghost"
                      type="button"
                      disabled={busy || !editBody.trim()}
                      onClick={() => saveEdit(comment)}
                    >
                      Save
                    </Button>
                  </div>
                </div>
              ) : (
                <SocialTextBody appearance="standard"
                  body={comment.body}
                  mentions={comment.mentions}
                  base={api.replace(/^\/api\/v1/, "")}
                  doc={
                    comment.richContent?.blocks[0]?.type === "text"
                      ? comment.richContent.blocks[0].doc
                      : null
                  }
                />
              )}
              {comment.richContent?.blocks
                .filter((block) => block.type === "image")
                .map((image) => (
                  <figure
                    key={image.slotId}
                    className="beta-post-inline-image"
                    style={{ maxWidth: `${image.width ?? 100}%` }}
                  >
                    {/* eslint-disable-next-line @next/next/no-img-element */}
                    <img
                      src={`${api}/attachments/${image.attachmentId}/content?variant=preview`}
                      alt={image.alt || image.name}
                      loading="lazy"
                      decoding="async"
                      width={900}
                      height={600}
                    />
                  </figure>
                ))}
              {post.agentReplies
                .filter(
                  (reply) =>
                    reply.owner_type === "comment" &&
                    reply.owner_id === comment.id &&
                    reply.state !== "completed",
                )
                .map((reply) => (
                  <SocialAgentStatus
                    key={reply.id}
                    reply={reply}
                    api={api}
                    userId={userId}
                    onChanged={onChanged}
                    aiName={post.aiName}
                    settingsUrl={settingsUrl}
                  />
                ))}
              <Button appearance="standard"
                variant="ghost"
                type="button"
                onClick={() => focusReplyComposer({ id: comment.id, name: comment.author_name })}
              >
                Reply to {comment.author_name}
              </Button>
              {replyTo?.id === comment.id && composer()}
            </div>
          </div>
        ))}
        {comments.length < post.commentCount && cursor !== null && (
          <Button appearance="standard" type="submit" variant="ghost" disabled={busy} onClick={loadOlder}>
            Load older comments
          </Button>
        )}
        {!replyTo && composer()}
      </div>
      <InlineNotice tone="info" role="status">{message}</InlineNotice>
    </>
  );
}
