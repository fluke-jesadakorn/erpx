"use client";
import type { SelfServiceSeed } from "@/server/self-service";

import { AnnouncementFeed } from "@/components/calendar/announcement-feed";
import { workFetch } from "@/lib/work-fetch";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import { MyDayCard } from "@/components/self-service/home";
import { workAttention, type WorkAttention as Attention } from "@/lib/work-attention";
import { WorkAttention } from "@/components/workplace/work-attention";
import { isMyAction, workInView, type WorkItem } from "@/lib/work-item";
import { WorkList } from "@/components/workplace/work-list";
import { StartActions } from "@/components/today/start-actions";
import { DailyPlan } from "@/components/today/daily-plan";
import { HandoverComposer } from "@/components/today/handover-composer";
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
import { MessagesSquare } from "lucide-react";
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
  const [attentionUpdatedAt, setAttentionUpdatedAt] = useState(0);
  const [workItems, setWorkItems] = useState<WorkItem[] | null>(null);
  const [todayView, setTodayView] = useState<"feed" | "actions" | "tracking" | "team">("feed");
  const [toolsOpen, setToolsOpen] = useState(false);
  const [pendingPosts, setPendingPosts] = useState<HomePost[] | null>(null);
  useEffect(() => {
    const openComposer = () => {
      if (["#compose", "#handover"].includes(window.location.hash)) setTodayView("feed");
    };
    openComposer();
    window.addEventListener("hashchange", openComposer);
    return () => window.removeEventListener("hashchange", openComposer);
  }, []);
  useEffect(() => {
    if (todayView === "feed" && ["#compose", "#handover"].includes(window.location.hash)) {
      const id = window.location.hash.slice(1);
      const frame = window.requestAnimationFrame(() => document.getElementById(id)?.scrollIntoView({ block: "start" }));
      return () => window.cancelAnimationFrame(frame);
    }
  }, [todayView]);
  useEffect(() => {
    const revealPlan = () => {
      if (window.location.hash === "#plan") setToolsOpen(true);
    };
    revealPlan();
    window.addEventListener("hashchange", revealPlan);
    return () => window.removeEventListener("hashchange", revealPlan);
  }, []);
  useEffect(() => {
    if (!toolsOpen || window.location.hash !== "#plan") return;
    const frame = window.requestAnimationFrame(() => document.getElementById("plan")?.scrollIntoView({ block: "start" }));
    return () => window.cancelAnimationFrame(frame);
  }, [toolsOpen]);
  const attentionRequest = useRef<AbortController | null>(null);
  const feedRef = useRef<HTMLElement>(null);
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
        setWorkItems(data.items);
        setAttention(workAttention(data.items, !!data.unavailableSources.length || !!data.capabilities.unavailable.length));
        setAttentionUpdatedAt(Date.now());
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
    if (single) return;
    // Synchronize the external Work snapshot after hydration; keep the feed available while loading.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void refreshPriorities();
    const stop = scheduleWorkRefresh(() => void refreshPriorities());
    return () => { attentionRequest.current?.abort(); stop(); };
  }, [refreshPriorities, single]);
  useEffect(() => {
    if (single) return;
    let active = true;
    const check = async () => {
      if (document.visibilityState !== "visible") return;
      try {
        const rows = await betaFetch<HomePost[]>(`${api}/beta/home?${query}&limit=${feedPageSize}`);
        if (active && rows.length && !posts.some((post) => post.id === rows[0].id)) setPendingPosts(rows);
      } catch { /* The current feed stays available while polling fails. */ }
    };
    const interval = window.setInterval(() => void check(), 60_000);
    return () => { active = false; window.clearInterval(interval); };
  }, [api, query, posts, single]);
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
    setPendingPosts(null);
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
  const visibleWork = todayView === "actions" ? workInView(workItems ?? [], "actions")
    : todayView === "tracking" ? [...new Map([
        ...workInView(workItems ?? [], "waiting"),
        ...workInView(workItems ?? [], "requests").filter((item) => item.queue !== "completed"),
      ].map((item) => [`${item.type}:${item.id}`, item])).values()]
    : workInView(workItems ?? [], "team");
  return (
    <HomeVisualPrototype surface="workspace" compactHero={!single}>
    <main className={`content ${styles.home}`}>
      <header className="beta-heading">
        <div>
          <span className="beta-hero-kicker">Your workspace <span aria-hidden="true">/</span> Today</span>
          <h1>
            {single ? "Team conversation" : "Today"}
          </h1>
          <p>{single ? "Follow this conversation." : `Good to see you, ${name.split(" ")[0]}. Here is what needs you and what your team is doing.`}</p>
        </div>
        <div className="beta-hero-art" aria-hidden="true">
          <span className="beta-hero-orbit" />
          <span className="beta-hero-pane beta-hero-pane-back" />
          <span className="beta-hero-pane beta-hero-pane-front" />
        </div>
        {single ? (
          <Link href={`${base}/home`}>Back to Today</Link>
        ) : null}
      </header>
      {!single && <div className={styles.priorityBand} id="home-priorities">
        <WorkAttention base={base} attention={attention} prominent loading={attentionLoading} snapshotAt={attentionUpdatedAt} retry={() => void refreshPriorities()} />
      </div>}
      {!single && <div className={styles.mobileSummary} aria-label="Today at a glance">
        <button id="today-tools-toggle" type="button" aria-expanded={toolsOpen} aria-controls="today-tools" onClick={() => {
          setToolsOpen(true);
          window.dispatchEvent(new Event("erpx:open-plan"));
          window.requestAnimationFrame(() => {
            const plan = document.getElementById("plan");
            plan?.scrollIntoView({ block: "start", behavior: "smooth" });
            plan?.focus({ preventScroll: true });
          });
        }}>Plan my day</button>
      </div>}
      <div className={single ? styles.singleLayout : styles.homeLayout}>
        {!single && <div className={styles.feedControls}>
          <nav className={styles.todayTabs} aria-label="Today views">
            {([ ["feed", "Feed"], ["actions", "My actions"], ["tracking", "Tracking"], ["team", "Team"] ] as const).map(([id,label]) => <button key={id} type="button" aria-pressed={todayView === id} onClick={() => setTodayView(id)}>{label}</button>)}
          </nav>
          {todayView === "feed" && <>
          <div id="compose">
          <SocialComposer
            api={api}
            name={name}
            branchId={branchId}
            meta={meta}
            setMeta={setMeta}
            loadMeta={() => { void loadMeta(); }}
            onPublished={refresh}
          />
          </div>
          <HandoverComposer base={base} onPublished={refresh} />
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
          </>}
        </div>}
        {!single && <aside className={styles.todayRail} aria-label="Today tools" id="today-tools" data-open={toolsOpen} tabIndex={-1}>
          <button type="button" className={styles.closeTools} onClick={() => {
            setToolsOpen(false);
            window.requestAnimationFrame(() => document.getElementById("today-tools-toggle")?.focus());
          }}>Close today&apos;s tools</button>
          <div className={styles.railStart}><StartActions base={base} rail /></div>
          <MyDayCard key={base} base={base} enabled={calendarEnabled} initial={initialDay} plan={<DailyPlan base={base} />} />
        </aside>}
        {todayView !== "feed" && !single ? <div className={styles.todayWork}>
          {workItems === null && attentionLoading ? <p role="status">Loading work…</p> : <WorkList base={base} items={visibleWork.slice(0, 30)} returnTo={`view=${todayView === "tracking" ? "waiting" : todayView}`} incomplete={attention.incomplete} />}
          <Link href={`${base}/work?view=${todayView === "tracking" ? "waiting" : todayView}`}>Open all {todayView === "actions" ? "actions" : todayView === "tracking" ? "tracking" : "team work"} →</Link>
        </div> : <div className={styles.stream}>
      <section ref={feedRef} className="beta-feed" aria-label="Team updates" tabIndex={-1}>
        {!single && calendarEnabled && <AnnouncementFeed base={base} />}
        {!single && pendingPosts && <button className={styles.newUpdates} type="button" onClick={() => { setPosts(pendingPosts); setPendingPosts(null); }}>New updates · Show them</button>}
        {!posts.length && (
          <div className="beta-empty">
            <MessagesSquare className="beta-empty-icon" size={23} aria-hidden="true" />
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
            onDeleted={() => {
              setPosts((rows) => rows.filter((row) => row.id !== post.id));
              setPendingPosts((rows) => {
                if (!rows) return null;
                const remaining = rows.filter((row) => row.id !== post.id);
                return remaining.length ? remaining : null;
              });
              setMessage("Post deleted.");
              window.requestAnimationFrame(() => feedRef.current?.focus({ preventScroll: true }));
              void refresh().catch(() => { /* The committed deletion remains reflected locally. */ });
            }}
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
        </div>}
      </div>
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
