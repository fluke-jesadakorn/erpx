"use client";

import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import { Card, InlineNotice, PageContent, Stack } from "@erpx/ui/components/foundation";
import { Button } from "@erpx/ui/components/button";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";

type FeedComment = { id: string; authorName: string; body: string };
type FeedPost = { id: string; authorName: string; body: string; createdAt: number; comments: FeedComment[] };
type FeedResult = { data: FeedPost[]; nextCursor: string | null; hasMore: boolean };
type Filter = "all" | "following" | "team" | "records";

const time = new Intl.DateTimeFormat("en-GB", { dateStyle: "medium", timeStyle: "short", timeZone: "Asia/Bangkok" });

async function responseData<T>(response: Response): Promise<T> {
  const payload = await response.json();
  if (!response.ok) throw new Error(payload.error?.message ?? "The request could not be completed.");
  return payload as T;
}

export function OasisFeed({ api }: { api: string }) {
  const [filter, setFilter] = useState<Filter>("all");
  const [result, setResult] = useState<FeedResult | null>(null);
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [replyId, setReplyId] = useState<string | null>(null);
  const pending = useRef(new Map<string, string>());
  const requests = useRef(0);

  const load = useCallback(async (cursor?: string) => {
    const request = ++requests.current;
    setLoading(true);
    setError("");
    const search = new URLSearchParams({ filter });
    if (cursor) search.set("cursor", cursor);
    try {
      const next = await responseData<FeedResult>(await workFetch(`${api}/feed?${search}`));
      if (request !== requests.current) return;
      setResult(previous => cursor && previous
        ? { ...next, data: [...new Map([...previous.data, ...next.data].map(item => [item.id, item])).values()] }
        : next);
    } catch (cause) {
      if (request === requests.current) setError(cause instanceof Error ? cause.message : "Unable to load Feed.");
    } finally {
      if (request === requests.current) setLoading(false);
    }
  }, [api, filter]);

  useEffect(() => {
    setResult(null);
    void load();
    return () => { requests.current++; };
  }, [load]);

  async function submit(event: FormEvent<HTMLFormElement>, postId?: string) {
    event.preventDefault();
    if (saving) return;
    const form = event.currentTarget;
    const body = String(new FormData(form).get("body") ?? "").trim();
    if (!body) return;
    const path = postId ? `/posts/${encodeURIComponent(postId)}/comments` : "/feed";
    const payload = JSON.stringify(postId ? { body } : { body, audience: "holding" });
    const command = path + payload;
    const key = pending.current.get(command) ?? crypto.randomUUID();
    pending.current.set(command, key);
    setSaving(true);
    setError("");
    try {
      await responseData(await workFetch(api + path, {
        method: "POST",
        headers: { "content-type": "application/json", "idempotency-key": key },
        body: payload,
      }));
      pending.current.delete(command);
      form.reset();
      if (postId) setReplyId(null);
      await load();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to save.");
    } finally {
      setSaving(false);
    }
  }

  return <PageContent>
    <Stack>
      <h1>Feed</h1>
      <Card appearance="standard">
        <form onSubmit={submit}>
          <Stack>
            <label htmlFor="oasis-post">Share an update</label>
            <textarea className="ui-input" id="oasis-post" name="body" rows={3} maxLength={4000} required />
            <div><Button appearance="standard" type="submit" clientManaged disabled={saving}>Post update</Button></div>
          </Stack>
        </form>
      </Card>
      <label>Show <select value={filter} disabled={saving} onChange={event => setFilter(event.target.value as Filter)}>
        <option value="all">All updates</option>
        <option value="following">Following</option>
        <option value="team">Team conversations</option>
        <option value="records">Linked records</option>
      </select></label>
      {error && <InlineNotice tone="danger">{error} <Button appearance="standard" variant="ghost" onClick={() => void load()}>Reload Feed</Button></InlineNotice>}
      {loading && !result && <p role="status">Loading team updates…</p>}
      {result?.data.length === 0 && <p>No updates in this view.</p>}
      {result?.data.map(post => <Card as="article" appearance="standard" key={post.id}>
        <Stack>
          <header><strong>{post.authorName}</strong> <time dateTime={new Date(post.createdAt).toISOString()}>{time.format(post.createdAt)}</time></header>
          <p style={{ whiteSpace: "pre-wrap" }}>{post.body}</p>
          {post.comments.length > 0 && <ul aria-label="Replies">{post.comments.map(comment => <li key={comment.id}><strong>{comment.authorName}</strong> {comment.body}</li>)}</ul>}
          {replyId === post.id ? <form onSubmit={event => void submit(event, post.id)}>
            <Stack>
              <label htmlFor={`reply-${post.id}`}>Reply to {post.authorName}</label>
              <textarea className="ui-input" id={`reply-${post.id}`} name="body" rows={2} maxLength={4000} required />
              <div><Button appearance="standard" type="submit" clientManaged disabled={saving}>Send reply</Button></div>
            </Stack>
          </form> : <div><Button appearance="standard" variant="ghost" onClick={() => setReplyId(post.id)}>Reply</Button></div>}
        </Stack>
      </Card>)}
      {result?.hasMore && result.nextCursor && <div><Button appearance="standard" variant="ghost" disabled={loading} onClick={() => void load(result.nextCursor!)}>Load more</Button></div>}
    </Stack>
  </PageContent>;
}
