"use client";
import { InlineNotice } from "@erpx/ui/components/foundation";

import { FormDisclosure } from "@erpx/ui/components/form-disclosure";
import { Button, Field, Select, TextArea, TextInput } from "@erpx/ui/components/foundation";

import { useCallback, useEffect, useRef, useState, type FormEvent } from "react";
import Link from "@erpx/workplace/components/workspace-link";
import { workFetch } from "@erpx/feature-work_projection/lib/work-fetch";
import type { InboxItem } from "@erpx/server-collaboration/public/inbox";
import styles from "@erpx/feature-collaboration/components/messaging/inbox-updates.module.css";
import { TimeText } from "@erpx/feature-people_payroll/components/time/time-preference-context";
type Person = { id: string; name: string; email: string };
type Node = { id: string; kind: string; name: string; inScope?: boolean };
type SentNotice = {
  id: string;
  title: string;
  accepted_count: number;
  delivered_count: number;
  created_at: number;
};
export type InitialInboxUpdates={items:InboxItem[];nextCursor:string|null;unread:number};
export function InboxUpdates({ base, canBroadcast,notificationsOnly=false,initial }: { base: string; canBroadcast: boolean;notificationsOnly?:boolean;initial?:InitialInboxUpdates }) {
  const initialKey=JSON.stringify({base,view:"unread",category:"",query:""});
  const [loadedKey,setLoadedKey]=useState(initial?initialKey:""),generation=useRef(0);
  const [items, setItems] = useState<InboxItem[]>(initial?.items??[]),
    [cursor, setCursor] = useState<string | null>(initial?.nextCursor??null),
    [view, setView] = useState("unread"),
    [category, setCategory] = useState(""),
    [search, setSearch] = useState(""),
    [query, setQuery] = useState(""),
    [people, setPeople] = useState<Person[]>([]),
    [nodes, setNodes] = useState<Node[]>([]),
    [sent, setSent] = useState<SentNotice[]>([]),
    [error, setError] = useState(""),
    [status, setStatus] = useState(""),
    [busy, setBusy] = useState(false),
    [unread, setUnread] = useState(initial?.unread??0);
  const invalidateRead=useCallback(()=>{++generation.current;},[]);
  const api = `/api/v1${base}/inbox`,readKey=JSON.stringify({base,view,category,query});
  const load = useCallback(
    async (next?: string | null, append = false) => {
      const request=++generation.current;
      const params = new URLSearchParams({
        view,
        ...(category ? { category } : {}),
        ...(query ? { search: query } : {}),
        ...(next ? { cursor: next } : {}),
      });
      const response = await workFetch(`${api}/notifications?${params}`, { cache: "no-store" });
      const json = await response.json();
      if (!response.ok) throw new Error(json.error?.message ?? "Could not load updates.");
      const summary = await workFetch(`${api}/summary`, { cache: "no-store" });
      const counts=summary.ok?await summary.json():null;
      if(request!==generation.current)return;
      setItems((old) => (append ? [...old, ...json.data.items] : json.data.items));
      setCursor(json.data.nextCursor);setLoadedKey(readKey);
      if(counts)setUnread(counts.data.unread);
    },
    [api, view, category, query,readKey],
  );
  useEffect(() => {
    if(loadedKey===readKey)return invalidateRead;
    const timer = window.setTimeout(() => {
      void load().catch((cause) =>
        setError(cause instanceof Error ? cause.message : "Could not load updates."),
      );
    }, 0);
    return () => {window.clearTimeout(timer);invalidateRead();};
  }, [load,loadedKey,readKey,invalidateRead]);
  useEffect(() => {
    const refresh = () => {
      if (document.visibilityState === "visible") void load().catch(() => {});
    };
    document.addEventListener("visibilitychange", refresh);
    window.addEventListener("focus", refresh);
    return () => {
      document.removeEventListener("visibilitychange", refresh);
      window.removeEventListener("focus", refresh);
    };
  }, [load]);
  useEffect(() => {
    if(notificationsOnly)return;
    void workFetch(`${api}/people`)
      .then((r) => r.json())
      .then((j) => setPeople(Array.isArray(j.data) ? j.data : []))
      .catch(() => {});
    void workFetch(`${api}/notices`)
      .then((r) => r.json())
      .then((j) => setSent(Array.isArray(j.data) ? j.data : []))
      .catch(() => {});
    if (canBroadcast)
      void workFetch(`/api/v1${base}/calendar/hierarchy`)
        .then((r) => r.json())
        .then((j) =>
          setNodes(
            (j.data?.nodes ?? []).filter(
              (n: Node) =>
                n.inScope !== false && ["company", "branch", "department"].includes(n.kind),
            ),
          ),
        )
        .catch(() => {});
  }, [api, base, canBroadcast,notificationsOnly]);
  async function state(id: string, patch: { read?: boolean; archived?: boolean }) {
    setError("");
    const response = await workFetch(`${api}/notifications/${id}/state`, {
      method: "POST",
      headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
      body: JSON.stringify(patch),
    });
    const json = await response.json();
    if (!response.ok) {
      setError(json.error?.message ?? "Could not update item.");
      return;
    }
    await load();
  }
  async function send(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setBusy(true);
    setError("");
    setStatus("");
    const form = new FormData(event.currentTarget),
      scope = String(form.get("audience") ?? "");
    const body = scope
      ? {
          title: form.get("title"),
          body: form.get("body"),
          recipients: [],
          audience: { kind: scope.split(":")[0], id: scope.slice(scope.indexOf(":") + 1) },
        }
      : {
          title: form.get("title"),
          body: form.get("body"),
          recipients: form.getAll("recipients").map(String),
        };
    try {
      const response = await workFetch(`${api}/notices`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "Idempotency-Key": crypto.randomUUID() },
        body: JSON.stringify(body),
      });
      const json = await response.json();
      if (!response.ok) throw new Error(json.error?.message ?? "Notice could not be sent.");
      setStatus(
        `Accepted for ${json.data.recipientCount} recipient${json.data.recipientCount === 1 ? "" : "s"}.`,
      );
      event.currentTarget.reset();
      void workFetch(`${api}/notices`)
        .then((r) => r.json())
        .then((j) => setSent(Array.isArray(j.data) ? j.data : []))
        .catch(() => {});
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Notice could not be sent.");
    } finally {
      setBusy(false);
    }
  }
  return (
    <div className={styles.updates}>
      <div className={styles.toolbar}>
        <strong>Updates {unread > 0 && <span className={styles.badge}>{unread}</span>}</strong>
        <Select appearance="standard" aria-label="Read filter" value={view} onChange={(e) => setView(e.target.value)}>
          <option value="unread">Unread</option>
          <option value="all">All</option>
        </Select>
        <Select appearance="standard"
          aria-label="Category"
          value={category}
          onChange={(e) => setCategory(e.target.value)}
        >
          <option value="">Every category</option>
          <option value="work">Work</option>
          <option value="calendar">Calendar</option>
          <option value="mention">Mentions</option>
          <option value="notice">Notices</option>
          <option value="system">System</option>
        </Select>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            setQuery(search);
          }}
        >
          <TextInput appearance="standard"
            aria-label="Search updates"
            placeholder="Search updates"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
          <Button appearance="standard" clientManaged type="submit" variant="ghost">
            Search
          </Button>
        </form>
      </div>
      {error && <InlineNotice tone="danger" role="alert">{error}</InlineNotice>}
      {status && <InlineNotice tone="info" role="status">{status}</InlineNotice>}
      <div className={styles.list}>
        {items.length === 0 ? (
          <p>No matching updates.</p>
        ) : (
          items.map((item) => (
            <article key={item.id} className={styles.item} data-unread={!item.readAt}>
              <div>
                <span className={styles.category}>{item.category}</span>{" "}
                <TimeText appearance="standard" value={item.createdAt} />
              </div>
              {item.href ? (
                <Link href={`${base}${item.href}`}>{item.message}</Link>
              ) : (
                <p>{item.message}</p>
              )}
              <div className={styles.actions}>
                <Button appearance="standard"
                  type="submit"
                  variant="ghost"
                  onClick={() => void state(item.id, { read: !item.readAt })}
                >
                  {item.readAt ? "Mark unread" : "Mark read"}
                </Button>
                <Button appearance="standard"
                  type="submit"
                  variant="ghost"
                  onClick={() => void state(item.id, { archived: true })}
                >
                  Archive
                </Button>
              </div>
            </article>
          ))
        )}
      </div>
      {cursor && (
        <Button appearance="standard"
          type="submit"
          variant="ghost"
          onClick={() => void load(cursor, true).catch((cause) => setError(String(cause)))}
        >
          Load more updates
        </Button>
      )}
      {!notificationsOnly&&<FormDisclosure appearance="standard" className={styles.compose}>
        <summary>Send a notice</summary>
        <form onSubmit={send}>
          <Field appearance="standard">
            Title
            <TextInput appearance="standard" name="title" maxLength={160} required />
          </Field>
          <Field appearance="standard">
            Message
            <TextArea appearance="standard" name="body" maxLength={5000} required />
          </Field>
          <Field appearance="standard">
            Named recipients
            <Select appearance="standard" name="recipients" multiple size={Math.min(6, Math.max(2, people.length))}>
              {people.map((person) => (
                <option key={person.id} value={person.id}>
                  {person.name} · {person.email}
                </option>
              ))}
            </Select>
          </Field>
          {canBroadcast && (
            <Field appearance="standard">
              Or broadcast to
              <Select appearance="standard" name="audience" defaultValue="">
                <option value="">Named recipients above</option>
                {nodes.map((node) => (
                  <option key={node.id} value={`${node.kind}:${node.id}`}>
                    {node.name} · {node.kind}
                  </option>
                ))}
              </Select>
            </Field>
          )}
          <p>Notices are plain text. Delivery status does not show who read them.</p>
          <Button appearance="standard" clientManaged type="submit" variant="ghost" disabled={busy}>
            {busy ? "Sending…" : "Send notice"}
          </Button>
        </form>
      </FormDisclosure>}
      {!notificationsOnly&&sent.length > 0 && (
        <details className={styles.compose}>
          <summary>Sent notices</summary>
          <div className={styles.list}>
            {sent.map((notice) => (
              <p key={notice.id}>
                <strong>{notice.title}</strong> · {notice.delivered_count}/{notice.accepted_count}{" "}
                deliveries accepted · <TimeText appearance="standard" value={notice.created_at} />
              </p>
            ))}
          </div>
        </details>
      )}
    </div>
  );
}
