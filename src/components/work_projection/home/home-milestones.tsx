"use client";
import { useEffect, useState } from "react";
import { betaFetch } from "@/lib/shared/beta-client";
import { Button, InlineNotice } from "@/components/ui/foundation";
import type { HomePost } from "@/server/modules/collaboration/public/social-home";
import { SystemMilestones } from "@/components/collaboration/system-milestones";
export function HomeMilestones({ base, query, revision }: { base: string; query: string; revision: number }) {
  const [posts, setPosts] = useState<HomePost[] | null>(null);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  useEffect(() => {
    let active = true;
    const params = new URLSearchParams(query); params.set("stream", "milestones"); params.set("limit", "8");
    async function load() {
      try {
        const rows = await betaFetch<HomePost[]>(`/api/v1${base}/beta/home?${params}`);
        if (active) { setPosts(rows); setError(""); }
      } catch (cause) { if (active) setError(cause instanceof Error ? cause.message : "Milestones are unavailable."); }
    }
    const initial = setTimeout(() => void load(), 0);
    const timer = setInterval(() => { if (document.visibilityState === "visible") void load(); }, 60_000);
    return () => { active = false; clearTimeout(initial); clearInterval(timer); };
  }, [base, query, revision, retry]);
  if (error) return <InlineNotice tone="info" role="status">{error} <Button appearance="standard" variant="ghost" onClick={() => setRetry(value => value + 1)}>Try again</Button></InlineNotice>;
  return posts ? <SystemMilestones base={base} posts={posts} /> : <p role="status">Loading work milestones…</p>;
}
