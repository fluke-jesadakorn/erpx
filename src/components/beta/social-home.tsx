"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Select } from "@/components/ui/foundation";


import { AnnouncementFeed } from "@/components/calendar/announcement-feed";
import { workFetch } from "@/lib/work-fetch";
import { useNavigationState } from "@/components/workplace/navigation-counts";
import { useSearchParams } from "next/navigation";
import Link from "next/link";
import { ComposerLauncher, HandoverLauncher } from "@/components/home/tool-launchers";
import { DeferredRegion } from "@/components/home/deferred-region";
import { lazy, useCallback, useEffect, useRef, useState } from "react";
import { betaFetch } from "@/lib/beta-client";
import type { HomePost } from "@/server/modules/collaboration/public/social-home";
import type { Meta } from "./social-types";

import { SocialPost } from "./social-post";
import dynamic from "next/dynamic";
import { MessagesSquare } from "lucide-react";
const SocialTaskDialog = dynamic(() =>
  import("./social-task-dialog").then((module) => module.SocialTaskDialog),
);
import styles from "./social.module.css";
const HomeMilestones = lazy(() => import("@/components/home/home-milestones").then(module => ({ default: module.HomeMilestones })));
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
}) {
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    const invalidate = () => { alive.current = false; };
    window.addEventListener("erpx:work-context-changed", invalidate);
    return () => { invalidate(); window.removeEventListener("erpx:work-context-changed", invalidate); };
  }, []);
  const { refresh: refreshPriorities } = useNavigationState();
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
  const feedRef = useRef<HTMLElement>(null);
  const metaLoaded = useRef(false);
  const metaRequest = useRef<Promise<Meta> | null>(null);
  const loadedFeedQuery = useRef<string | null>(null);
  const searchParams = useSearchParams();
  const tag = searchParams.get("tag") ?? "";
  const api = `/api/v1${base}`;
  const [posts, setPosts] = useState(initialPosts),
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
  const [milestoneRevision, setMilestoneRevision] = useState(0);
  if (loadedFeedQuery.current === null) loadedFeedQuery.current = query;
  useEffect(() => {
    if (loadedFeedQuery.current === query) return;
    loadedFeedQuery.current = query;
    let current = true;
    betaFetch<HomePost[]>(`${api}/beta/home?${query}&limit=${feedPageSize}`)
      .then(rows => {
        if (current && alive.current) {
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
  useEffect(() => {
    if (single) return;
    let active = true;
    const check = async () => {
      if (document.visibilityState !== "visible") return;
      try {
        const rows = await betaFetch<HomePost[]>(`${api}/beta/home?${query}&limit=${feedPageSize}`);
        if (active && alive.current && rows.length && !posts.some((post) => post.id === rows[0].id))
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
  }, [api, query, posts, single]);
  const loadMeta = useCallback(() => {
    if (metaLoaded.current) return Promise.resolve();
    if (!metaRequest.current) {
      metaRequest.current = betaFetch<Meta>(`${api}/beta/meta`)
        .then((nextMeta) => {
          if (!alive.current) return nextMeta;
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
    const rows = await betaFetch<HomePost[]>(`${api}/beta/home?${query}&limit=${feedPageSize}`);
    if (!alive.current) return;
    if (loadedFeedQuery.current !== query) return;
    setPosts(rows);
    setMilestoneRevision(value => value + 1);
    setPendingPosts(null);
    setHasMore(rows.length === feedPageSize);
  }
  async function refreshPost(id: string) {
    const response = await workFetch(`${api}/posts/${id}`);
    if (!alive.current) return;
    if (response.status === 404) {
      setPosts((rows) => rows.filter((row) => row.id !== id));
      return;
    }
    const body = (await response.json()) as {
      data: HomePost;
      error?: { message: string };
    };
    if (!alive.current) return;
    if (!response.ok) throw Error(body.error?.message ?? "Unable to refresh this conversation.");
    setPosts((rows) => rows.map((row) => (row.id === id ? body.data : row)));
  }
  async function loadMore() {
    setLoading(true);
    setMessage("");
    const selection = query;
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
          ...(team ? { team } : {}),
        });
      const more = await betaFetch<HomePost[]>(`${api}/beta/home?${query}`);
      if (!alive.current || loadedFeedQuery.current !== selection) return;
      setPosts((rows) => [...new Map([...rows, ...more].map((row) => [row.id, row])).values()]);
      setHasMore(more.length === feedPageSize);
    } catch (e) {
      setMessage(e instanceof Error ? e.message : "Unable to load updates.");
    } finally {
      setLoading(false);
    }
  }
  return (
    <>
          {!single && (
            <div className={styles.feedControls}>
              <div id="compose">
                <ComposerLauncher
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
              <HandoverLauncher base={base} onPublished={refresh} />
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
            {!single && <DeferredRegion label="Recent work milestones" minHeight={80}>
              <HomeMilestones key={query} base={base} query={query} revision={milestoneRevision} />
            </DeferredRegion>}
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
    </>
  );
}
