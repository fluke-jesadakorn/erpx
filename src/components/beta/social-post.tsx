"use client";
import { PersonAvatar } from "@/components/ui/person-avatar";
import { Dialog } from "@/components/ui/dialog";
import { Alert, Button } from "@/components/ui/foundation";
import { SocialTextBody } from "./social-text-body";
import { SocialTextInput } from "./social-text-input";
import type { Mention } from "@/lib/social-text";
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
import type { HomePost } from "@/server/social-home";
import type { Meta } from "./social-types";
import { formatUiDateTime } from "@/lib/ui-format";
import { useSocialCommand } from "./social-command";
import { SocialComments } from "./social-comments";
import { HandoverAcknowledgement } from "@/components/today/handover-acknowledgement";

function PostImage({ src, alt }: { src: string; alt: string }) {
  const [failed, setFailed] = useState(false);
  return <a href={src} target="_blank" rel="noreferrer">
    {failed ? <span className="beta-image-missing">Image preview unavailable · Open {alt}</span>
      : <Image unoptimized width={900} height={600} src={src} alt={alt} loading="lazy"
          onLoad={event => { if (event.currentTarget.naturalWidth <= 1 || event.currentTarget.naturalHeight <= 1) setFailed(true); }}
          onError={() => setFailed(true)} />}
  </a>;
}

export function SocialPost({
  base,
  post,
  meta,
  userId,
  canModerate,
  single,
  refresh,
  onDeleted,
  prepare,
}: {
  base: string;
  post: HomePost;
  meta: Meta;
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
    [editBody, setEditBody] = useState(post.body),
    [editMentions, setEditMentions] = useState<Mention[]>(post.mentions),
    [deleteConfirm, setDeleteConfirm] = useState(false),
    [revisions, setRevisions] = useState<Array<{ prior_version: number; action: string; body: string; created_at: number }> | null>(null);
  const { run, send, busy, message, setMessage } = useSocialCommand();
  const action = (_post: HomePost, input: unknown) =>
    run(async () => {
      await send(`${api}/posts/${post.id}`, input);
      await refresh();
    });
  const pending = post.agentReplies.some(r=>["pending","running"].includes(r.state));
  const ownTeamPost = post.author_id === userId && post.kind === "team" && !post.linked_module;
  const canEditText = ownTeamPost && !post.files.length && !post.content?.some(block => block.type === "image");
  const purpose = post.purpose && post.purpose !== "update"
    ? post.purpose.charAt(0).toUpperCase() + post.purpose.slice(1) : "Update";
  useEffect(()=>{if(!pending)return;const timer=setInterval(()=>{void refresh().catch(()=>{});},3000);return ()=>clearInterval(timer);},[pending,refresh]);
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
      const mentions = post.mentions.filter(mention => mention.start >= offset && mention.end <= offset + block.text.length)
        .map(mention => ({ ...mention, start: mention.start - offset, end: mention.end - offset }));
      return <SocialTextBody key={`text-${index}`} body={block.text} mentions={mentions} base={base} />;
    }
    const file = post.files.find(item => item.id === block.attachmentId);
    if (!file) return null;
    return <figure className="beta-post-inline-image" key={block.slotId}>
      <PostImage src={`${api}/attachments/${file.id}/content`} alt={block.alt || file.original_name} />
    </figure>;
  });
  const hasText = post.content ? post.content.some(block => block.type === "text" && !!block.text.trim()) : !!post.body.trim();
  const photoOnly = post.files.length > 0 && !hasText;
  return (
    <article id={post.id} key={post.id} className="card beta-post">
      {post.agentReplies.filter(r=>r.state!=="completed").map(r=><div key={r.id} role="status">{r.state==="failed"?r.error:"ERPX assistant is preparing a reply…"}{r.state==="failed"&&r.actor_id===userId&&<button disabled={busy} onClick={()=>run(async()=>{await send(`/api/v1${base}/beta/agent-replies/${r.id}`,{});await refresh();})}>Retry assistant</button>}</div>)}
      <header className="beta-person">
        <PersonAvatar name={post.author_name} />
        <div>
          <strong>{post.author_name}</strong>
          <p>
            <Link href={`${base}/posts/${post.id}`}>
              {formatUiDateTime(post.created_at, "Asia/Bangkok", {
                day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit",
              })}
            </Link>{" "}
            ·{" "}
            {purpose} · {post.audienceLabel}
          </p>
        </div>
        <details ref={postMoreRef} className="post-more" name="beta-post-actions">
          <summary aria-label="More post actions">•••</summary>
          <div className="post-more-menu">
            {" "}
            <button
              className="beta-follow"
              aria-label={
                post.followed ? "Unfollow conversation" : "Follow conversation"
              }
              aria-pressed={!!post.followed}
              disabled={busy}
              onClick={() =>
                action(post, { action: "follow", active: !post.followed })
              }
            >
              <Bookmark size={17} />
              {post.followed ? "Unfollow conversation" : "Follow conversation"}
            </button>
            <button
              disabled={busy}
              onClick={() => prepare(post, "summary", false)}
            >
              <Sparkles size={16} /> Summarize
            </button>
            <button onClick={() => setReportOpen((value) => !value)}>
              <Flag size={16} />
              Report{canModerate ? " / Moderate" : ""}
            </button>
            {canEditText && <button disabled={busy} onClick={() => {
              setEditBody(post.body); setEditMentions(post.mentions); setEditing(true);
            }}><Pencil size={16} /> Edit post</button>}
            {ownTeamPost && <button disabled={busy} onClick={() => void run(async () => {
              setRevisions(await betaFetch(`${api}/posts/${post.id}/revisions`));
            })}><History size={16} /> Edit history</button>}
            {ownTeamPost && <button type="button" className="post-delete-action" disabled={busy} onClick={() => {
              if (postMoreRef.current) postMoreRef.current.open = false;
              setMessage("");
              setDeleteConfirm(true);
            }}><Trash2 size={16} /> Delete post</button>}
          </div>
        </details>
      </header>
      {photoOnly && <p className="beta-media-caption">Photo update · {post.files.length} {post.files.length === 1 ? "image" : "images"}</p>}
      {editing ? <div className="beta-post-editor" aria-label="Edit post">
        <SocialTextInput api={api} value={editBody} mentions={editMentions}
          onChange={(body, mentions) => { setEditBody(body); setEditMentions(mentions); }}
          maxLength={3000} label="Edit post message" placeholder="Update your message…" disabled={busy} />
        <div className="beta-composer-actions">
          <button className="button" disabled={busy || !editBody.trim()} onClick={() => void run(async () => {
            await send(`${api}/posts/${post.id}`, {
              action: "edit", expectedVersion: post.version, body: editBody, mentions: editMentions,
            });
            setEditing(false);
            await refresh();
          })}>Save changes</button>
          <button className="button secondary" disabled={busy} onClick={() => setEditing(false)}>Cancel</button>
        </div>
      </div> : <div className="beta-post-content">
        {post.content ? orderedContent : <SocialTextBody body={post.body} mentions={post.mentions} base={base}/>}
      </div>}
      {post.edited_at && !editing && <p className="small">Edited</p>}
      {revisions && <section className="beta-post-history" aria-label="Edit history">
        <strong>Edit history</strong>
        {revisions.length ? revisions.map(row => <div key={row.prior_version}>
          <small>{row.action === "edit" ? "Before edit" : "Before deletion"} · Version {row.prior_version} · {formatUiDateTime(row.created_at)}</small>
          <p>{row.body}</p>
        </div>) : <p>No earlier versions.</p>}
        <button onClick={() => setRevisions(null)}>Close history</button>
      </section>}
      <Dialog open={deleteConfirm} onClose={() => {
        if (busy) return;
        setDeleteConfirm(false);
        setMessage("");
        window.requestAnimationFrame(() => postMoreRef.current?.querySelector<HTMLElement>("summary")?.focus());
      }} title="Delete post?">
        <p>This post will disappear from the feed and its link will no longer open. Linked work remains available to its participants, and ERPX keeps an audit record of the deletion.</p>
        {message && <Alert tone="danger" title="Post could not be deleted" live>{message}</Alert>}
        <div className="beta-composer-actions beta-delete-actions">
          <Button variant="secondary" data-autofocus disabled={busy} onClick={() => {
            setDeleteConfirm(false);
            setMessage("");
            window.requestAnimationFrame(() => postMoreRef.current?.querySelector<HTMLElement>("summary")?.focus());
          }}>Keep post</Button>
          <Button variant="danger" pending={busy} onClick={() => void run(async () => {
            await send(`${api}/posts/${post.id}`, { action: "retract", expectedVersion: post.version });
            setDeleteConfirm(false);
            onDeleted();
            if (single) router.replace(`${base}/home`);
          })}>{busy ? "Deleting…" : "Delete post"}</Button>
        </div>
      </Dialog>
      {!post.content && <div className="beta-images">
        {post.files.map((f) => <PostImage key={f.id} src={`${api}/attachments/${f.id}/content`} alt={f.original_name} />)}
      </div>}
      {post.task ? (
        <Link
          className="beta-task-link"
          href={`${base}/staff-tasks/${post.task.id}`}
        >
          Linked work · {post.task.state.replaceAll("_", " ")}{" "}
          <ArrowUpRight size={15} />
        </Link>
      ) : post.trackingState ? (
        <p className="beta-task-link">
          Work progress · {post.trackingState.replaceAll("_", " ")}
        </p>
      ) : post.hasTask ? (
        <p className="small">
          A task is linked. Its details are available to its participants.
        </p>
      ) : null}
      {post.linkedSources.length > 0 && <div className="beta-post-sources" aria-label="Linked work">
        {post.linkedSources.map((source, index) => source.available
          ? <Link className="beta-task-link" key={`${source.kind}:${source.id}`} href={`${base}${source.href}`}>
              {source.kind === "staff_task" ? "Task" : "Project"} · {source.title} <ArrowUpRight size={15} />
            </Link>
          : <span className="small" key={`unavailable-${index}`}>Linked work unavailable to your role</span>)}
      </div>}
      {post.purpose === "handover" && post.audience_kind === "participants" && post.linkedSources.length > 0 &&
        <HandoverAcknowledgement api={api} postId={post.id} version={post.version} />}
      <div className="beta-post-actions">
        <button
          disabled={busy}
          aria-pressed={!!post.appreciated}
          onClick={() =>
            action(post, {
              action: "appreciate",
              active: !post.appreciated,
            })
          }
        >
          <Heart size={16} /> Thanks {post.appreciationCount || ""}
        </button>
        <button
          aria-expanded={commentsOpen}
          onClick={() => setCommentsOpen((value) => !value)}
        >
          <MessageCircle size={16} /> Comment {post.commentCount || ""}
        </button>

        {!post.hasTask && (
          <button disabled={busy} onClick={() => prepare(post, "task", false)}>
            <Plus size={16} /> Create task
          </button>
        )}
      </div>
      <div hidden={!commentsOpen}>
        <SocialComments
          api={api}
          post={post}
          meta={meta}
          userId={userId}
          onChanged={refresh}
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
          <input
            name="reason"
            placeholder="Reason for reporting"
            aria-label="Reason for reporting"
            required
            maxLength={2000}
          />
          <button disabled={busy}>Send report</button>
        </form>
        {canModerate && (
          <button
            disabled={busy}
            onClick={() => action(post, { action: "hide" })}
          >
            Hide this post
          </button>
        )}
      </details>
      {!deleteConfirm && <p role="status">{message}</p>}
    </article>
  );
}
