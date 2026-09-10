"use client";
import { SocialTextBody } from "./social-text-body";
import { useEffect, useState } from "react";
import Image from "next/image";
import Link from "next/link";
import {
  Heart,
  MessageCircle,
  Plus,
  Sparkles,
  Bookmark,
  ArrowUpRight,
} from "lucide-react";
import type { HomePost } from "@/server/social-home";
import type { Meta } from "./social-types";
import { useSocialCommand } from "./social-command";
import { SocialComments } from "./social-comments";
export function SocialPost({
  base,
  post,
  meta,
  userId,
  canModerate,
  single,
  refresh,
  prepare,
}: {
  base: string;
  post: HomePost;
  meta: Meta;
  userId: string;
  canModerate: boolean;
  single: boolean;
  refresh: () => Promise<void>;
  prepare: (post: HomePost, mode: "task" | "summary", useAi: boolean) => void;
}) {
  const api = `/api/v1${base}`,
    [commentsOpen, setCommentsOpen] = useState(single),
    [reportOpen, setReportOpen] = useState(false);
  const { run, send, busy, message } = useSocialCommand();
  const action = (_post: HomePost, input: unknown) =>
    run(async () => {
      await send(`${api}/posts/${post.id}`, input);
      await refresh();
    });
  const pending = post.agentReplies.some(r=>["pending","running"].includes(r.state));
  useEffect(()=>{if(!pending)return;const timer=setInterval(()=>{void refresh().catch(()=>{});},3000);return ()=>clearInterval(timer);},[pending,refresh]);
  return (
    <article id={post.id} key={post.id} className="card beta-post">
      {post.agentReplies.filter(r=>r.state!=="completed").map(r=><div key={r.id} role="status">{r.state==="failed"?r.error:"ERPX assistant is preparing a reply…"}{r.state==="failed"&&r.actor_id===userId&&<button disabled={busy} onClick={()=>run(async()=>{await send(`/api/v1${base}/beta/agent-replies/${r.id}`,{});await refresh();})}>Retry assistant</button>}</div>)}
      <header className="beta-person">
        <span className="avatar">{post.author_name.slice(0, 1)}</span>
        <div>
          <strong>{post.author_name}</strong>
          <p>
            <Link href={`${base}/posts/${post.id}`}>
              {new Date(post.created_at).toLocaleString("en-GB", {
                timeZone: "Asia/Bangkok",
              })}
            </Link>{" "}
            ·{" "}
            {post.branch_id
              ? (meta.branches.find((b) => b.id === post.branch_id)
                  ?.display_name ?? "Branch")
              : "Company"}
          </p>
        </div>
        <details className="post-more">
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
            </button>
            <button
              disabled={busy}
              onClick={() => prepare(post, "summary", false)}
            >
              <Sparkles size={16} /> Summarize
            </button>
            <button onClick={() => setReportOpen((value) => !value)}>
              Report{canModerate ? " / Moderate" : ""}
            </button>
          </div>
        </details>
      </header>
      <SocialTextBody body={post.body} mentions={post.mentions} base={base}/>
      <div className="beta-images">
        {post.files.map((f) => (
          <a
            key={f.id}
            href={`${api}/attachments/${f.id}/content`}
            target="_blank"
            rel="noreferrer"
          >
            <Image
              unoptimized
              width={900}
              height={600}
              src={`${api}/attachments/${f.id}/content`}
              alt={f.original_name}
              loading="lazy"
            />
          </a>
        ))}
      </div>
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
          <MessageCircle size={16} /> Comment {post.comments.length || ""}
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
      <p role="status">{message}</p>
    </article>
  );
}
