"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Select } from "@/components/ui/foundation";

import type { SelfServiceSeed } from "@/server/modules/records/public/self-service";

import { AnnouncementFeed } from "@/components/calendar/announcement-feed";
import { workFetch } from "@/lib/work-fetch";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import { MyDayCard } from "@/components/self-service/home";
import { workAttention, type WorkAttention as Attention } from "@/lib/work-attention";
import { WorkAttention } from "@/components/workplace/work-attention";
import { isMyAction } from "@/lib/work-item";
import { StartActions } from "@/components/today/start-actions";
import { DailyPlan } from "@/components/today/daily-plan";
import { HandoverComposer } from "@/components/today/handover-composer";
import type { WorkSummary } from "@/server/modules/work_projection/public/work-summary";
import { useCallback, useEffect, useRef, useState } from "react";
import { betaFetch } from "@/lib/beta-client";
import type { HomePost } from "@/server/modules/collaboration/public/social-home";
import type { Meta } from "./social-types";
import { SocialComposer } from "./social-composer";
import { SocialPost } from "./social-post";
import dynamic from "next/dynamic";
import { scheduleWorkRefresh } from "@/lib/work-refresh";
import { HomeVisualPrototype } from "@/components/home/home-visual-prototype";
import { MessagesSquare } from "lucide-react";
const SocialTaskDialog = dynamic(() =>
  import("./social-task-dialog").then((module) => module.SocialTaskDialog),
);
import styles from "./social.module.css";
import { SystemMilestones } from "./system-milestones";
const feedPageSize = 8;
export function SocialHome({
  base,
  userId,
  name,
  branchId,
  canModerate,
  initialPosts,
  initialMilestones = [],
  initialMeta,
  single = false,
  calendarEnabled = false,
  initialDay,
  canRequestLegal = false,
}: {
  base: string;
  userId: string;
  name: string;
  branchId: string | null;
  canModerate: boolean;
  initialPosts: HomePost[];
  initialMilestones?: HomePost[];
  initialMeta: Meta;
  single?: boolean;
  calendarEnabled?: boolean;
  initialDay?: Promise<SelfServiceSeed | null>;
  canRequestLegal?: boolean;
}) {
  const [attention, setAttention] = useState<Attention>(() => workAttention([]));
  const [attentionLoading, setAttentionLoading] = useState(!single);
  const [attentionUpdatedAt, setAttentionUpdatedAt] = useState(0);
  const [toolsOpen, setToolsOpen] = useState(false);
  const [pendingPosts, setPendingPosts] = useState<HomePost[] | null>(null);
  useEffect(() => {
    const openComposer = () => {
      if (["#compose", "#handover"].includes(window.location.hash)) {
        const id = window.location.hash.slice(1);
        window.requestAnimationFrame(() =>
          document.getElementById(id)?.scrollIntoView({ block: "start" }),
        );
      }
    };
    openComposer();
    window.addEventListener("hashchange", openComposer);
    return () => window.removeEventListener("hashchange", openComposer);
  }, []);
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
    const frame = window.requestAnimationFrame(() =>
      document.getElementById("plan")?.scrollIntoView({ block: "start" }),
    );
    return () => window.cancelAnimationFrame(frame);
  }, [toolsOpen]);
  const attentionRequest = useRef<AbortController | null>(null);
  const feedRef = useRef<HTMLElement>(null);
  const metaLoaded = useRef(false);
  const metaRequest = useRef<Promise<Meta> | null>(null);
  const loadedFeedQuery = useRef<string | null>(null);
  const searchParams = useSearchParams();
  const tag = searchParams.get("tag") ?? "";
  const api = `/api/v1${base}`;
  const [posts, setPosts] = useState(initialPosts),
    [milestones, setMilestones] = useState(initialMilestones),
    [meta, setMeta] = useState(initialMeta),
    [filter, setFilter] = useState("all"),
    [branch, setBranch] = useState(""),
    [team, setTeam] = useState("");
  const [message, setMessage] = useState(""),
    [loading, setLoading] = useState(false),
    [hasMore, setHasMore] = useState(initialPosts.length === feedPageSize);
  const [selected, setSelected] = useState<HomePost | null>(null),
    [mode, setMode] = useState<"task" | "summary">("task"),
    [dialogOpen, setDialogOpen] = useState(false);
  const postId = single ? initialPosts[0]?.id : undefined;
  const query = postId
    ? `post=${encodeURIComponent(postId)}`
    : new URLSearchParams({
        filter,
        stream: "conversations",
        ...(tag ? { tag } : {}),
        ...(branch ? { branch } : {}),
        ...(team ? { team } : {}),
      }).toString();
  const milestoneParams = new URLSearchParams(query);
  milestoneParams.set("stream", "milestones");
  const milestoneQuery = milestoneParams.toString();
  if (loadedFeedQuery.current === null) loadedFeedQuery.current = query;
  useEffect(() => {
    if (loadedFeedQuery.current === query) return;
    loadedFeedQuery.current = query;
    let current = true;
    Promise.all([
      betaFetch<HomePost[]>(`${api}/beta/home?${query}&limit=${feedPageSize}`),
      betaFetch<HomePost[]>(`${api}/beta/home?${milestoneQuery}&limit=${feedPageSize}`),
    ])
      .then(([rows, systemRows]) => {
        if (current) {
          setPosts(rows);
          setMilestones(systemRows);
          setHasMore(rows.length === feedPageSize);
        }
      })
      .catch((e) => {
        if (current) setMessage(e.message);
      });
    return () => {
      current = false;
    };
  }, [api, query, milestoneQuery]);
  const refreshPriorities = useCallback(async () => {
    attentionRequest.current?.abort();
    const request = new AbortController();
    attentionRequest.current = request;
    setAttentionLoading(true);
    try {
      const response = await workFetch(`${api}/work/summary`, {
        cache: "no-store",
        signal: request.signal,
      });
      if ([401, 403, 404].includes(response.status)) {
        if (!request.signal.aborted) setAttention(workAttention([], true));
        return;
      }
      if (!response.ok) throw new Error("Work unavailable");
      const { data } = (await response.json()) as { data: WorkSummary };
      if (!request.signal.aborted) {
        setAttention(
          workAttention(
            data.items,
            !!data.unavailableSources.length || !!data.capabilities.unavailable.length,
          ),
        );
        setAttentionUpdatedAt(Date.now());
        window.dispatchEvent(
          new CustomEvent("erpx:work-summary", {
            detail: {
              work:
                data.unavailableSources.length || data.capabilities.unavailable.length
                  ? null
                  : data.items.filter(isMyAction).length,
            },
          }),
        );
      }
    } catch {
      if (!request.signal.aborted) setAttention((current) => ({ ...current, incomplete: true }));
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
    return () => {
      attentionRequest.current?.abort();
      stop();
    };
  }, [refreshPriorities, single]);
  useEffect(() => {
    if (single) return;
    let active = true;
    const check = async () => {
      if (document.visibilityState !== "visible") return;
      try {
        const [rows, systemRows] = await Promise.all([
          betaFetch<HomePost[]>(`${api}/beta/home?${query}&limit=${feedPageSize}`),
          betaFetch<HomePost[]>(`${api}/beta/home?${milestoneQuery}&limit=${feedPageSize}`),
        ]);
        if (active) setMilestones(systemRows);
        if (active && rows.length && !posts.some((post) => post.id === rows[0].id))
          setPendingPosts(rows);
      } catch {
        /* The current feed stays available while polling fails. */
      }
    };
    const interval = window.setInterval(() => void check(), 60_000);
    return () => {
      active = false;
      window.clearInterval(interval);
    };
  }, [api, query, milestoneQuery, posts, single]);
  const loadMeta = useCallback(() => {
    if (metaLoaded.current) return Promise.resolve();
    if (!metaRequest.current) {
      metaRequest.current = betaFetch<Meta>(`${api}/beta/meta`)
        .then((nextMeta) => {
          metaLoaded.current = true;
          setMeta(nextMeta);
          return nextMeta;
        })
        .finally(() => {
          metaRequest.current = null;
        });
    }
    return metaRequest.current;
  }, [api, setMeta]);
  async function refresh() {
    if (!single) void refreshPriorities();
    const [rows, systemRows] = await Promise.all([
      betaFetch<HomePost[]>(`${api}/beta/home?${query}&limit=${feedPageSize}`),
      single
        ? Promise.resolve([])
        : betaFetch<HomePost[]>(`${api}/beta/home?${milestoneQuery}&limit=${feedPageSize}`),
    ]);
    setPosts(rows);
    setMilestones(systemRows);
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
    if (!response.ok) throw Error(body.error?.message ?? "Unable to refresh this conversation.");
    setPosts((rows) => rows.map((row) => (row.id === id ? body.data : row)));
  }
  async function loadMore() {
    setLoading(true);
    setMessage("");
    try {
      const last = posts[posts.length - 1],
        query = new URLSearchParams({
          filter,
          stream: "conversations",
          ...(tag ? { tag } : {}),
          before: String(last.created_at),
          beforeId: last.id,
          limit: String(feedPageSize),
          ...(branch ? { branch } : {}),
        });
      const more = await betaFetch<HomePost[]>(`${api}/beta/home?${query}`);
      setPosts((rows) => [...new Map([...rows, ...more].map((row) => [row.id, row])).values()]);
      setHasMore(more.length === feedPageSize);
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Unable to load updates.");
    } finally {
      setLoading(false);
    }
  }
  return (
    <HomeVisualPrototype surface="workspace" compactHero={!single}>
      <div className={`content ${styles.home}`}>
        <header className="beta-heading">
          <div>
            <span className="beta-hero-kicker">
              Your workspace <span aria-hidden="true">/</span> Today
            </span>
            <h1>{single ? "Team conversation" : "Today"}</h1>
            <p>
              {single
                ? "Follow this conversation."
                : `Good to see you, ${name.split(" ")[0]}. Catch up with your team.`}
            </p>
          </div>
          <div className="beta-hero-art" aria-hidden="true">
            <span className="beta-hero-orbit" />
            <span className="beta-hero-pane beta-hero-pane-back" />
            <span className="beta-hero-pane beta-hero-pane-front" />
          </div>
          {single ? <Link href={`${base}/home`}>Back to Today</Link> : null}
        </header>
        {!single && (
          <div className={styles.mobileSummary} aria-label="Today at a glance">
            <Link href={`${base}/work?view=actions`}>
              Needs you{" "}
              <strong>
                {attentionLoading ? "…" : `${attention.total}${attention.incomplete ? "+" : ""}`}
              </strong>
            </Link>
            {calendarEnabled && <Link href={`${base}/calendar`}>Calendar</Link>}
            <Button appearance="standard"
              variant="ghost"
              id="today-tools-toggle"
              type="button"
              aria-expanded={toolsOpen}
              aria-controls="today-tools"
              onClick={() => {
                setToolsOpen(true);
                window.dispatchEvent(new Event("erpx:open-plan"));
                window.requestAnimationFrame(() => {
                  const plan = document.getElementById("plan");
                  plan?.scrollIntoView({ block: "start", behavior: "smooth" });
                  plan?.focus({ preventScroll: true });
                });
              }}
            >
              Plan my day
            </Button>
          </div>
        )}
        {!single && attention.incomplete && (
          <InlineNotice tone="info" className={styles.mobileNotice} role="status">
            Some work is missing or out of date. <Link href={`${base}/work`}>Check My work</Link>
          </InlineNotice>
        )}
        <div className={single ? styles.singleLayout : styles.homeLayout}>
          {!single && (
            <div className={styles.feedControls}>
              <div id="compose">
                <SocialComposer
                  api={api}
                  name={name}
                  branchId={branchId}
                  meta={meta}
                  setMeta={setMeta}
                  loadMeta={() => {
                    void loadMeta();
                  }}
                  onPublished={refresh}
                />
              </div>
              <HandoverComposer base={base} onPublished={refresh} />
              {tag && (
                <p>
                  Hashtag: #{tag} · <Link href={`${base}/home`}>Clear hashtag</Link>
                </p>
              )}
              <nav className="beta-filters" aria-label="Filter conversations">
                <Button appearance="standard"
                  type="submit"
                  variant="ghost"
                  aria-pressed={filter === "all"}
                  onClick={() => setFilter("all")}
                >
                  Latest updates
                </Button>
                <Button appearance="standard"
                  type="submit"
                  variant="ghost"
                  aria-pressed={filter === "following"}
                  onClick={() => setFilter("following")}
                >
                  Following
                </Button>
                {meta.audiences?.some((audience) => audience.kind === "team") && (
                  <Select appearance="standard"
                    aria-label="Filter team"
                    value={team}
                    onChange={(e) => setTeam(e.target.value)}
                  >
                    <option value="">All accessible teams</option>
                    {meta.audiences
                      .filter((audience) => audience.kind === "team")
                      .map((audience) => (
                        <option key={audience.value} value={audience.value.slice("team:".length)}>
                          {audience.label}
                        </option>
                      ))}
                  </Select>
                )}
                {meta.branches.length > 1 && (
                  <Select appearance="standard"
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
                  </Select>
                )}
              </nav>
            </div>
          )}
          {!single && (
            <aside
              className={styles.todayRail}
              aria-label="Today tools"
              id="today-tools"
              data-open={toolsOpen}
              tabIndex={-1}
            >
              <Button appearance="standard"
                variant="ghost"
                type="button"
                className={styles.closeTools}
                onClick={() => {
                  setToolsOpen(false);
                  window.requestAnimationFrame(() =>
                    document.getElementById("today-tools-toggle")?.focus(),
                  );
                }}
              >
                Close today&apos;s tools
              </Button>
              <WorkAttention
                base={base}
                attention={{ ...attention, items: attention.items.slice(0, 2) }}
                compact
                rail
                loading={attentionLoading}
                snapshotAt={attentionUpdatedAt}
                retry={() => void refreshPriorities()}
              />
              <div className={styles.railStart}>
                <StartActions base={base} rail canRequestLegal={canRequestLegal} />
              </div>
              <MyDayCard
                key={base}
                base={base}
                enabled={calendarEnabled}
                initial={initialDay}
                plan={<DailyPlan base={base} />}
              />
            </aside>
          )}
          <div className={styles.stream}>
            <section ref={feedRef} className="beta-feed" aria-label="Team updates" tabIndex={-1}>
              {!single && calendarEnabled && <AnnouncementFeed base={base} />}
              {!single && pendingPosts && (
                <Button appearance="standard"
                  variant="ghost"
                  className={styles.newUpdates}
                  type="button"
                  onClick={() => {
                    setPosts(pendingPosts);
                    setPendingPosts(null);
                  }}
                >
                  New updates · Show them
                </Button>
              )}
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
                    window.requestAnimationFrame(() =>
                      feedRef.current?.focus({ preventScroll: true }),
                    );
                    void refresh().catch(() => {
                      /* The committed deletion remains reflected locally. */
                    });
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
            <InlineNotice tone="info" role="status">{message}</InlineNotice>
            {!single && hasMore && posts.length > 0 && (
              <Button appearance="standard"
                type="submit"
                variant="secondary"

                disabled={loading}
                onClick={() => void loadMore()}
              >
                Load {feedPageSize} older updates
              </Button>
            )}
            {!single && <SystemMilestones base={base} posts={milestones} />}
          </div>
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
      </div>
    </HomeVisualPrototype>
  );
}
