"use client";
import type { SelfServiceSeed } from "@/server/self-service";

import { AnnouncementFeed } from "@/components/calendar/announcement-feed";
import { workFetch } from "@/lib/work-fetch";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import { SelfServiceHome } from "@/components/self-service/home";
import { workAttention, type WorkAttention as Attention } from "@/lib/work-attention";
import { WorkAttention } from "@/components/workplace/work-attention";
import { isMyAction } from "@/lib/work-item";
import type { WorkSummary } from "@/server/work-summary";
import { useCallback, useEffect, useRef, useState } from "react";
import { betaFetch } from "@/lib/beta-client";
import type { HomePost } from "@/server/social-home";
import type { Meta } from "./social-types";
import { SocialComposer } from "./social-composer";
import { SocialPost } from "./social-post";
import dynamic from "next/dynamic";
import { scheduleWorkRefresh } from "@/lib/work-refresh";
import { HomeVisualPrototype } from "@/components/home/home-visual-prototype";
const SocialTaskDialog = dynamic(() => import("./social-task-dialog").then(module => module.SocialTaskDialog));
import styles from "./social.module.css";
const feedPageSize = 8;
export function SocialHome({
  base,
  userId,
  name,
  branchId,
  canModerate,
  initialPosts,
  initialMeta,
  single = false,
  calendarEnabled = false,
  initialDay,
}: {
  base: string;
  userId: string;
  name: string;
  branchId: string | null;
  canModerate: boolean;
  initialPosts: HomePost[];
  initialMeta: Meta;
  single?: boolean;
  calendarEnabled?: boolean;
  initialDay?: Promise<SelfServiceSeed | null>;
}) {
  const [attention, setAttention] = useState<Attention>(() => workAttention([]));
  const [attentionLoading, setAttentionLoading] = useState(!single);
  const attentionRequest = useRef<AbortController | null>(null);
  const metaLoaded = useRef(false);
  const metaRequest = useRef<Promise<Meta> | null>(null);
  const loadedFeedQuery = useRef<string | null>(null);
  const searchParams=useSearchParams();
  const tag=searchParams.get("tag") ?? "";
  const api = `/api/v1${base}`;
  const [posts, setPosts] = useState(initialPosts),
    [meta, setMeta] = useState(initialMeta),
    [filter, setFilter] = useState("all"),
    [branch, setBranch] = useState("");
  const [message, setMessage] = useState(""),
    [loading, setLoading] = useState(false),
    [hasMore, setHasMore] = useState(initialPosts.length === feedPageSize);
  const [selected, setSelected] = useState<HomePost | null>(null),
    [mode, setMode] = useState<"task" | "summary">("task"),
    [dialogOpen, setDialogOpen] = useState(false);
  const postId = single ? initialPosts[0]?.id : undefined;
  const query = postId
    ? `post=${encodeURIComponent(postId)}`
    : new URLSearchParams({ filter, ...(tag ? {tag}: {}), ...(branch ? { branch } : {}) }).toString();
  if (loadedFeedQuery.current === null) loadedFeedQuery.current = query;
  useEffect(() => {
    if (loadedFeedQuery.current === query) return;
    loadedFeedQuery.current = query;
    let current = true;
    betaFetch<HomePost[]>(`${api}/beta/home?${query}&limit=${feedPageSize}`)
      .then((rows) => {
        if (current) {
          setPosts(rows);
          setHasMore(rows.length === feedPageSize);
        }
      })
      .catch((e) => {
        if (current) setMessage(e.message);
      });
    return () => {
      current = false;
    };
  }, [api, query]);
  const refreshPriorities = useCallback(async () => {
    attentionRequest.current?.abort();
    const request = new AbortController();
    attentionRequest.current = request;
    setAttentionLoading(true);
    try {
      const response = await workFetch(`${api}/work/summary`, { cache: "no-store", signal: request.signal });
      if ([401, 403, 404].includes(response.status)) {
        if (!request.signal.aborted) setAttention(workAttention([], true));
        return;
      }
      if (!response.ok) throw new Error("Work unavailable");
      const { data } = await response.json() as { data: WorkSummary };
      if (!request.signal.aborted) {
        setAttention(workAttention(data.items, !!data.unavailableSources.length || !!data.capabilities.unavailable.length));
        window.dispatchEvent(new CustomEvent("erpx:work-summary", { detail: {
          work: data.unavailableSources.length || data.capabilities.unavailable.length ? null : data.items.filter(isMyAction).length,
        } }));
      }
    } catch {
      if (!request.signal.aborted) setAttention(current => ({ ...current, incomplete: true }));
    } finally {
      if (attentionRequest.current === request) setAttentionLoading(false);
    }
  }, [api]);
  useEffect(() => {
    if (single || calendarEnabled) return;
    // Synchronize the external Work snapshot after hydration; keep the feed available while loading.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void refreshPriorities();
    const stop = scheduleWorkRefresh(() => void refreshPriorities());
    return () => { attentionRequest.current?.abort(); stop(); };
  }, [refreshPriorities, single, calendarEnabled]);
  const loadMeta = useCallback(() => {
    if (metaLoaded.current) return Promise.resolve();
    if (!metaRequest.current) {
      metaRequest.current = betaFetch<Meta>(`${api}/beta/meta`)
        .then((nextMeta) => {
          metaLoaded.current = true;
          setMeta(nextMeta);
          return nextMeta;
        })
        .finally(() => { metaRequest.current = null; });
    }
    return metaRequest.current;
  }, [api, setMeta]);
  async function refresh() {
    if (!single) void refreshPriorities();
    const rows = await betaFetch<HomePost[]>(`${api}/beta/home?${query}&limit=${feedPageSize}`);
    setPosts(rows);
    setHasMore(rows.length === feedPageSize);
  }
  async function refreshPost(id: string) {
    const response = await workFetch(`${api}/posts/${id}`);
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
          limit: String(feedPageSize),
          ...(branch ? { branch } : {}),
        });
      const more = await betaFetch<HomePost[]>(`${api}/beta/home?${query}`);
      setPosts((rows) => [
        ...new Map([...rows, ...more].map((row) => [row.id, row])).values(),
      ]);
      setHasMore(more.length === feedPageSize);
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Unable to load updates.");
    } finally {
      setLoading(false);
    }
  }
  return (
    <HomeVisualPrototype surface="workspace">
    <main className={`content ${styles.home}`}>
      <header className="beta-heading">
        <div>
          <h1>
            {single ? "Team conversation" : `Hello, ${name.split(" ")[0]}`}
          </h1>
          <p>Your work and team updates, in one place.</p>
        </div>
        {single ? (
          <Link href={`${base}/home`}>Back to Home</Link>
        ) : null}
      </header>
      <SelfServiceHome key={base} base={base} enabled={!single && calendarEnabled} initial={initialDay}>
      <div className={single ? styles.singleLayout : calendarEnabled ? undefined : styles.homeLayout}>
        {!single && !calendarEnabled && <aside className={styles.attentionColumn}>
          <WorkAttention base={base} attention={attention} loading={attentionLoading} retry={() => void refreshPriorities()} />
        </aside>}
        <div className={styles.stream}>
      {!single && (
        <>
          <SocialComposer
            api={api}
            name={name}
            branchId={branchId}
            meta={meta}
            setMeta={setMeta}
            loadMeta={() => { void loadMeta(); }}
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
        {!single && calendarEnabled && <AnnouncementFeed base={base} />}
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
              void loadMeta();
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
          Load {feedPageSize} older updates
        </button>
      )}
        </div>
      </div>
      </SelfServiceHome>
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
    </HomeVisualPrototype>
  );
}
