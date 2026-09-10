"use client";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import type { WorkItem } from "@/lib/work-item";
import { useEffect, useState } from "react";
import { betaFetch } from "@/lib/beta-client";
import type { HomePost } from "@/server/social-home";
import type { Meta } from "./social-types";
import { SocialComposer } from "./social-composer";
import { SocialPost } from "./social-post";
import { SocialTaskDialog } from "./social-task-dialog";
import styles from "./social.module.css";
export function SocialHome({
  base,
  userId,
  name,
  branchId,
  canModerate,
  initialPosts,
  initialMeta,
  single = false,
  nextSteps: initialNextSteps = [],
  unavailableWork: initialUnavailableWork = [],
}: {
  base: string;
  userId: string;
  name: string;
  branchId: string | null;
  canModerate: boolean;
  initialPosts: HomePost[];
  initialMeta: Meta;
  single?: boolean;
  nextSteps?: WorkItem[];
  unavailableWork?: string[];
}) {
  const [nextSteps, setNextSteps] = useState(initialNextSteps);
  const [unavailableWork, setUnavailableWork] = useState(
    initialUnavailableWork,
  );
  const searchParams=useSearchParams();
  const tag=searchParams.get("tag") ?? "";
  const api = `/api/v1${base}`;
  const [posts, setPosts] = useState(initialPosts),
    [meta, setMeta] = useState(initialMeta),
    [filter, setFilter] = useState("all"),
    [branch, setBranch] = useState("");
  const [message, setMessage] = useState(""),
    [loading, setLoading] = useState(false),
    [hasMore, setHasMore] = useState(initialPosts.length === 20);
  const [selected, setSelected] = useState<HomePost | null>(null),
    [mode, setMode] = useState<"task" | "summary">("task"),
    [dialogOpen, setDialogOpen] = useState(false);
  const postId = single ? initialPosts[0]?.id : undefined;
  const query = postId
    ? `post=${encodeURIComponent(postId)}`
    : new URLSearchParams({ filter, ...(tag ? {tag}: {}), ...(branch ? { branch } : {}) }).toString();
  useEffect(() => {
    let current = true;
    betaFetch<HomePost[]>(`${api}/beta/home?${query}`)
      .then((rows) => {
        if (current) {
          setPosts(rows);
          setHasMore(rows.length === 20);
        }
      })
      .catch((e) => {
        if (current) setMessage(e.message);
      });
    return () => {
      current = false;
    };
  }, [api, query]);
  async function refreshPriorities() {
    try {
      const work = await betaFetch<{
        nextSteps: WorkItem[];
        unavailableSources: string[];
      }>(`${api}/work/summary`);
      setNextSteps(work.nextSteps);
      setUnavailableWork(work.unavailableSources);
    } catch {
      setUnavailableWork(["Next actions"]);
    }
  }
  async function refresh() {
    void refreshPriorities();
    const [rows, nextMeta] = await Promise.all([
      betaFetch<HomePost[]>(`${api}/beta/home?${query}`),
      betaFetch<Meta>(`${api}/beta/meta`),
    ]);
    setPosts(rows);
    setHasMore(rows.length === 20);
    setMeta(nextMeta);
  }
  async function refreshPost(id: string) {
    const response = await fetch(`${api}/posts/${id}`);
    if (response.status === 404) {
      setPosts((rows) => rows.filter((row) => row.id !== id));
      return;
    }
    const body = (await response.json()) as {
      data: HomePost;
      error?: { message: string };
    };
    if (!response.ok)
      throw Error(
        body.error?.message ?? "Unable to refresh this conversation.",
      );
    setPosts((rows) => rows.map((row) => (row.id === id ? body.data : row)));
  }
  async function loadMore() {
    setLoading(true);
    setMessage("");
    try {
      const last = posts[posts.length - 1],
        query = new URLSearchParams({
          filter,
          ...(tag ? {tag}: {}),
          before: String(last.created_at),
          beforeId: last.id,
          ...(branch ? { branch } : {}),
        });
      const more = await betaFetch<HomePost[]>(`${api}/beta/home?${query}`);
      setPosts((rows) => [
        ...new Map([...rows, ...more].map((row) => [row.id, row])).values(),
      ]);
      setHasMore(more.length === 20);
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Unable to load updates.");
    } finally {
      setLoading(false);
    }
  }
  return (
    <main className={`content ${styles.home}`}>
      <header className="beta-heading">
        <div>
          <h1>
            {single ? "Team conversation" : `Hello, ${name.split(" ")[0]}`}
          </h1>
          <p>Share an update. Move work forward.</p>
        </div>
        {single && <Link href={`${base}/home`}>Back to Home</Link>}
      </header>
      {!single && (
        <>
          <section className="beta-priorities" aria-label="Your work">
            <div>
              <strong>Your next steps</strong>
              <Link href={`${base}/work`}>All work →</Link>
            </div>
            {nextSteps.map((item) => (
              <Link
                key={`${item.type}:${item.id}`}
                href={`${base}${item.href}`}
              >
                <span>{item.title}</span>
                <small>{item.nextAction ?? item.detail}</small>
              </Link>
            ))}
            {!nextSteps.length && !unavailableWork.length && (
              <p>You’re all caught up.</p>
            )}
            {!!unavailableWork.length && (
              <p role="status">
                Some work is temporarily unavailable.{" "}
                <Link href={`${base}/work`}>Open Work to retry.</Link>
              </p>
            )}
          </section>
          <SocialComposer
            api={api}
            name={name}
            branchId={branchId}
            meta={meta}
            setMeta={setMeta}
            onPublished={refresh}
          />
          {tag&&<p>Hashtag: #{tag} · <Link href={`${base}/home`}>Clear hashtag</Link></p>}
          <nav className="beta-filters" aria-label="Filter conversations">
            <button
              aria-pressed={filter === "all"}
              onClick={() => setFilter("all")}
            >
              Latest updates
            </button>
            <button
              aria-pressed={filter === "following"}
              onClick={() => setFilter("following")}
            >
              Following
            </button>
            {meta.branches.length > 1 && (
              <select
                aria-label="Filter branch"
                value={branch}
                onChange={(e) => setBranch(e.target.value)}
              >
                <option value="">All accessible branches</option>
                {meta.branches.map((b) => (
                  <option key={b.id} value={b.id}>
                    {b.display_name}
                  </option>
                ))}
              </select>
            )}
          </nav>
        </>
      )}
      <section className="beta-feed" aria-label="Team updates">
        {!posts.length && (
          <div className="card">
            <h2>No conversations here yet</h2>
            <p>Share an update or try another filter.</p>
          </div>
        )}
        {posts.map((post) => (
          <SocialPost
            key={post.id}
            base={base}
            post={post}
            meta={meta}
            userId={userId}
            canModerate={canModerate}
            single={single}
            refresh={() => refreshPost(post.id)}
            prepare={(post, mode) => {
              setSelected(post);
              setMode(mode);
              setDialogOpen(true);
            }}
          />
        ))}
      </section>
      <p role="status">{message}</p>
      {!single && hasMore && posts.length > 0 && (
        <button
          className="button secondary"
          disabled={loading}
          onClick={() => void loadMore()}
        >
          Load older updates
        </button>
      )}
      {selected && (
        <SocialTaskDialog
          key={selected.id}
          base={base}
          selected={selected}
          meta={meta}
          userId={userId}
          mode={mode}
          open={dialogOpen}
          onClose={() => {
            setDialogOpen(false);
            void refreshPriorities();
          }}
        />
      )}
    </main>
  );
}
