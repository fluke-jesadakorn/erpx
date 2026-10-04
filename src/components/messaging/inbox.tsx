"use client";
import { InlineNotice } from "@/components/ui/foundation";

import { Button, Field, Select, TextArea, TextInput } from "@/components/ui/foundation";

import { Dialog } from "@/components/ui/dialog";
import { TimeText } from "@/components/time/time-preference-context";
import { FeatureTabBar, FeatureWidget, WidgetGrid } from "@/components/features/feature-page";

import { PersonAvatar } from "@/components/ui/person-avatar";
import { workFetch } from "@/lib/work-fetch";
import Image from "next/image";
import { useRouter } from "next/navigation";

import type {
  MessagingHistory,
  MessagingMessage,
  MessagingOverview,
  MessagingPerson,
  MessagingRoom,
  MessagingTeam,
} from "@/lib/messaging";
import {
  ArrowLeft,
  Hash,
  LockKeyhole,
  MessageCircle,
  Paperclip,
  Plus,
  Send,
  Settings,
  Users,
  X,
} from "lucide-react";
import { useCallback, useEffect, useRef, useState, type FormEvent, type ReactNode } from "react";
import styles from "./inbox.module.css";

async function api<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await workFetch(url, { ...init, cache: "no-store" });
  const result = await response.json();
  if (!response.ok)
    throw new Error(result.error?.message ?? "Unable to connect. Please try again.");
  return result.data as T;
}
type MessageFileUpload = {
  id: string;
  attachmentLinkId?: string;
  state?: string;
};

function messageFileMediaType(file: File) {
  if (file.type) return file.type;
  const lower = file.name.toLowerCase();
  if (lower.endsWith(".heic")) return "image/heic";
  if (lower.endsWith(".heif")) return "image/heif";
  return "text/csv";
}

async function waitForMessageFile(root: string, upload: MessageFileUpload) {
  if (!upload.attachmentLinkId || upload.state === "available") return;
  const attachmentRoot = root.replace(/\/messaging$/, "");
  for (let attempt = 0; attempt < 120; attempt += 1) {
    const status = await api<{
      sourceState: string | null;
      scan?: { lastError?: string | null };
    }>(`${attachmentRoot}/attachment-links/${encodeURIComponent(upload.attachmentLinkId)}/status`);
    if (status.sourceState === "available") return;
    if (["malware_rejected", "scan_error", "destroyed"].includes(status.sourceState ?? ""))
      throw new Error(
        status.sourceState === "malware_rejected"
          ? "The attachment was rejected by malware scanning."
          : (status.scan?.lastError ?? "The attachment could not be scanned."),
      );
    await new Promise((resolve) => setTimeout(resolve, 750));
  }
  throw new Error("The attachment scan is still pending. Try sending again shortly.");
}
function post<T>(url: string, input: unknown, key = crypto.randomUUID()) {
  return api<T>(url, {
    method: "POST",
    headers: { "Content-Type": "application/json", "Idempotency-Key": key },
    body: JSON.stringify(input),
  });
}
type Editor = {
  kind: "direct" | "group" | "team" | "channel";
  team?: MessagingTeam;
  room?: MessagingRoom;
  editing?: boolean;
};

export function MessagingInbox({
  base,
  userId,
  updates,
  initialTab,
  initialPerson,
  initialRoom,
  notificationUnread = 0,
}: {
  base: string;
  userId: string;
  updates: ReactNode;
  initialTab: "updates" | "chats" | "teams";
  initialPerson?: string;
  initialRoom?: string;
  notificationUnread?: number;
}) {
  const router = useRouter();
  const root = `/api/v1${base}/messaging`;
  const tab = initialTab;
  const [overview, setOverview] = useState<MessagingOverview>({
    teams: [],
    rooms: [],
  });
  const [selected, setSelected] = useState<string | null>(initialRoom ?? null);
  const [editor, setEditor] = useState<Editor | null>(null);
  const [error, setError] = useState(""),
    [loading, setLoading] = useState(true);
  const refresh = useCallback(async () => {
    const result = await api<MessagingOverview>(root);
    setOverview(result);
    setLoading(false);
    setError("");
    setSelected((current) =>
      current && result.rooms.some((r) => r.id === current) ? current : null,
    );
  }, [root]);
  useEffect(() => {
    if (!initialPerson || initialPerson === userId) return;
    let stopped = false;
    void post<{ id: string }>(`${root}/rooms`, { kind: "direct", members: [initialPerson] })
      .then(async (result) => {
        await refresh();
        if (!stopped) setSelected(result.id);
      })
      .catch((e) => {
        if (!stopped) setError((e as Error).message);
      });
    return () => {
      stopped = true;
    };
  }, [root, initialPerson, userId, refresh]);
  useEffect(() => {
    if (tab === "updates") return;
    let stopped = false,
      busy = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      if (busy || stopped) return;
      busy = true;
      try {
        if (document.visibilityState === "visible") await refresh();
      } catch (e) {
        if (!stopped) {
          setError((e as Error).message);
          setLoading(false);
        }
      } finally {
        busy = false;
        if (!stopped) timer = setTimeout(poll, 2000);
      }
    }
    const visible = () => {
      clearTimeout(timer);
      void poll();
    };
    void poll();
    document.addEventListener("visibilitychange", visible);
    return () => {
      stopped = true;
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", visible);
    };
  }, [refresh, tab]);
  const room = overview.rooms.find((r) => r.id === selected);
  const chats = overview.rooms.filter((r) => r.kind !== "channel");
  function choose(id: string) {
    setSelected(id);
  }
  function roomButton(r: MessagingRoom) {
    return (
      <Button appearance="standard"
        type="submit"
        variant="ghost"
        key={r.id}
        className={`${styles.room} ${selected === r.id ? styles.selected : ""}`}
        onClick={() => choose(r.id)}
        aria-current={selected === r.id ? "true" : undefined}
      >
        <span className={styles.roomIcon}>
          {r.kind === "channel" ? (
            r.private ? (
              <LockKeyhole size={18} />
            ) : (
              <Hash size={18} />
            )
          ) : r.kind === "group" ? (
            <Users size={19} />
          ) : (
            <PersonAvatar name={r.name} />
          )}
        </span>
        <span className={styles.roomText}>
          <strong>{r.name}</strong>
          <small>{r.preview ?? "Start the conversation"}</small>
        </span>
        {r.unread > 0 && (
          <span className={styles.badge} aria-label={`${r.unread} unread`}>
            {r.unread}
          </span>
        )}
      </Button>
    );
  }
  return (
    <>
      <FeatureTabBar appearance="standard" aria-label="Inbox sections">
        {(["chats", "teams", "updates"] as const).map((value) => (
          <Button appearance="standard"
            type="submit"
            variant="ghost"
            key={value}
            aria-current={tab === value ? "page" : undefined}
            onClick={() => {
              router.push(`${base}/inbox?tab=${value}`, { scroll: false });
            }}
          >
            {{ chats: "Messages", teams: "Teams", updates: "Notifications" }[value]}
            {value === "updates" && notificationUnread > 0 && (
              <span className={styles.badge}>{notificationUnread}</span>
            )}
            {value !== "updates" &&
              overview.rooms
                .filter((r) => (value === "teams") === (r.kind === "channel"))
                .reduce((sum, r) => sum + r.unread, 0) > 0 && (
                <span className={styles.badge}>
                  {overview.rooms
                    .filter((r) => (value === "teams") === (r.kind === "channel"))
                    .reduce((sum, r) => sum + r.unread, 0)}
                </span>
              )}
          </Button>
        ))}
      </FeatureTabBar>
      {tab === "updates" ? (
        updates
      ) : (
        <>
          {error && (
            <InlineNotice tone="info" className={styles.error} role="status">
              {error} Reconnecting…
            </InlineNotice>
          )}
          <WidgetGrid pageKey={`inbox-${tab}`}>
            <FeatureWidget appearance="standard" id="inbox-conversations" title={tab === "chats" ? "Messages" : "Teams"}>
              <section className={`${styles.workspace} ${room ? styles.hasRoom : ""}`}>
                <aside
                  className={styles.sidebar}
                  aria-label={tab === "chats" ? "Conversations" : "Teams and channels"}
                >
                  <div className={styles.sidebarHeader}>
                    <h2>{tab === "chats" ? "Chats" : "Your teams"}</h2>
                    <Button appearance="standard"
                      type="submit"
                      variant="ghost"
                      className={styles.iconButton}
                      aria-label={tab === "chats" ? "New chat" : "New team"}
                      disabled={loading}
                      onClick={() => setEditor({ kind: tab === "chats" ? "direct" : "team" })}
                    >
                      <Plus size={20} />
                    </Button>
                  </div>
                  {loading ? (
                    <p className={styles.hint}>Loading conversations…</p>
                  ) : tab === "chats" ? (
                    <>
                      {chats.map(roomButton)}
                      {!chats.length && (
                        <div className={styles.emptyList}>
                          <MessageCircle />
                          <p>Your conversations start here.</p>
                          <Button appearance="standard"
                            type="submit"
                            variant="ghost"
                            className={styles.mobileStart}
                            onClick={() => setEditor({ kind: "direct" })}
                          >
                            Start a chat
                          </Button>
                        </div>
                      )}
                    </>
                  ) : (
                    <>
                      {overview.teams.map((t) => (
                        <section key={t.id} className={styles.team}>
                          <div className={styles.teamHeader}>
                            <strong>{t.name}</strong>
                            {!t.organization_managed && t.owner_id === userId && (
                              <span>
                                <Button appearance="standard"
                                  type="submit"
                                  variant="ghost"
                                  aria-label={`Add channel to ${t.name}`}
                                  onClick={() => setEditor({ kind: "channel", team: t })}
                                >
                                  <Plus size={16} />
                                </Button>
                                <Button appearance="standard"
                                  type="submit"
                                  variant="ghost"
                                  aria-label={`Manage ${t.name}`}
                                  onClick={() =>
                                    setEditor({
                                      kind: "team",
                                      team: t,
                                      editing: true,
                                    })
                                  }
                                >
                                  <Settings size={16} />
                                </Button>
                              </span>
                            )}
                          </div>
                          {overview.rooms.filter((r) => r.team_id === t.id).map(roomButton)}
                        </section>
                      ))}
                      {!overview.teams.length && (
                        <div className={styles.emptyList}>
                          <Users />
                          <p>A space for every team.</p>
                          <Button appearance="standard"
                            type="submit"
                            variant="ghost"
                            onClick={() => setEditor({ kind: "team" })}
                          >
                            Create a team
                          </Button>
                        </div>
                      )}
                    </>
                  )}
                </aside>
                {room ? (
                  <Conversation
                    key={room.id}
                    root={root}
                    room={room}
                    userId={userId}
                    onBack={() => setSelected(null)}
                    onRead={() => {
                      void refresh().catch(() => {});
                    }}
                    onManage={() =>
                      setEditor({
                        kind: room.kind === "channel" ? "channel" : "group",
                        room,
                        team: overview.teams.find((t) => t.id === room.team_id),
                        editing: true,
                      })
                    }
                  />
                ) : (
                  <div className={styles.welcome}>
                    <span className={styles.welcomeIcon}>
                      {tab === "teams" ? <Users size={32} /> : <MessageCircle size={32} />}
                    </span>
                    <h2>
                      {tab === "teams" ? "Work together, in one place" : "A conversation away"}
                    </h2>
                    <p>
                      {tab === "teams"
                        ? "Choose a channel to connect with your team."
                        : "Choose a chat or start a new conversation with someone in your company."}
                    </p>
                    <Button appearance="standard"
                      type="submit"
                      variant="primary"
                      className={styles.primary}
                      disabled={loading}
                      onClick={() => setEditor({ kind: tab === "teams" ? "team" : "direct" })}
                    >
                      <Plus size={16} />
                      {tab === "teams" ? "Create team" : "New conversation"}
                    </Button>
                  </div>
                )}
              </section>
            </FeatureWidget>
          </WidgetGrid>
        </>
      )}
      {editor && (
        <ConversationEditor
          key={`${editor.kind}-${editor.room?.id ?? editor.team?.id ?? ""}`}
          root={root}
          userId={userId}
          editor={editor}
          onClose={() => setEditor(null)}
          onSaved={async (id) => {
            await refresh();
            setEditor(null);
            if (id) setSelected(id);
          }}
        />
      )}
    </>
  );
}

function Conversation({
  root,
  room,
  userId,
  onBack,
  onRead,
  onManage,
}: {
  root: string;
  room: MessagingRoom;
  userId: string;
  onBack: () => void;
  onRead: () => void;
  onManage: () => void;
}) {
  const [messages, setMessages] = useState<MessagingMessage[]>([]),
    [hasOlder, setHasOlder] = useState(false),
    [loading, setLoading] = useState(true),
    [error, setError] = useState("");
  const [body, setBody] = useState(""),
    [attachments, setAttachments] = useState<{ id: string; file: File }[]>([]),
    [sending, setSending] = useState(false),
    [sendError, setSendError] = useState("");
  const last = useRef(0),
    read = useRef(0),
    initialized = useRef(false),
    scroller = useRef<HTMLDivElement>(null),
    stick = useRef(true),
    attempt = useRef<{ signature: string; key: string } | null>(null);
  const onReadRef = useRef(onRead);
  useEffect(() => {
    onReadRef.current = onRead;
  }, [onRead]);
  const merge = useCallback((incoming: MessagingMessage[]) => {
    setMessages((current) => {
      const map = new Map(current.map((m) => [m.id, m]));
      incoming.forEach((m) => map.set(m.id, m));
      return [...map.values()].sort((a, b) => a.seq - b.seq);
    });
    if (incoming.length) last.current = Math.max(last.current, ...incoming.map((m) => m.seq));
  }, []);
  useEffect(() => {
    let stopped = false,
      busy = false;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      if (busy || stopped) return;
      busy = true;
      try {
        if (document.visibilityState === "visible") {
          const history = await api<MessagingHistory>(
            `${root}/rooms/${room.id}/messages${initialized.current ? `?after=${last.current}` : ""}`,
          );
          if (stopped) return;
          if (!initialized.current) {
            setHasOlder(history.hasMore);
            initialized.current = true;
          }
          merge(history.messages);
          setError("");
          setLoading(false);
          if (
            document.visibilityState === "visible" &&
            stick.current &&
            last.current > read.current
          ) {
            const seen = last.current;
            await post(`${root}/rooms/${room.id}/read`, { seq: seen });
            read.current = seen;
            onReadRef.current();
          }
        }
      } catch (e) {
        if (!stopped) {
          setError((e as Error).message);
          setLoading(false);
        }
      } finally {
        busy = false;
        if (!stopped) timer = setTimeout(poll, 2000);
      }
    }
    const visible = () => {
      clearTimeout(timer);
      void poll();
    };
    void poll();
    document.addEventListener("visibilitychange", visible);
    return () => {
      stopped = true;
      clearTimeout(timer);
      document.removeEventListener("visibilitychange", visible);
    };
  }, [root, room.id, merge]);
  useEffect(() => {
    if (stick.current && scroller.current)
      scroller.current.scrollTop = scroller.current.scrollHeight;
  }, [messages]);
  async function older() {
    const element = scroller.current,
      height = element?.scrollHeight ?? 0;
    try {
      stick.current = false;
      const history = await api<MessagingHistory>(
        `${root}/rooms/${room.id}/messages?before=${messages[0]?.seq ?? 0}`,
      );
      merge(history.messages);
      setHasOlder(history.hasMore);
      requestAnimationFrame(() => {
        if (element) element.scrollTop += element.scrollHeight - height;
      });
    } catch (e) {
      setError((e as Error).message);
    }
  }
  async function send(event: FormEvent) {
    event.preventDefault();
    if (sending || (!body.trim() && !attachments.length)) return;
    const signature = JSON.stringify({
      body: body.trim(),
      files: attachments.map((a) => a.id),
    });
    if (attempt.current?.signature !== signature)
      attempt.current = { signature, key: crypto.randomUUID() };
    setSending(true);
    setSendError("");
    try {
      for (const attachment of attachments) {
        const upload = await api<MessageFileUpload>(
          `${root}/rooms/${room.id}/files/${attachment.id}`,
          {
            method: "PUT",
            headers: {
              "Content-Type": messageFileMediaType(attachment.file),
              "X-File-Name": encodeURIComponent(attachment.file.name),
            },
            body: attachment.file,
          },
        );
        await waitForMessageFile(root, upload);
      }
      await post(`${root}/rooms/${room.id}/messages`, JSON.parse(signature), attempt.current.key);
      attempt.current = null;
      setBody("");
      setAttachments([]);
      stick.current = true;
      try {
        const history = await api<MessagingHistory>(
          `${root}/rooms/${room.id}/messages?after=${last.current}`,
        );
        merge(history.messages);
      } catch (e) {
        setError((e as Error).message);
      }
    } catch (e) {
      setSendError((e as Error).message);
    } finally {
      setSending(false);
    }
  }
  return (
    <section className={styles.conversation} aria-label={`Conversation: ${room.name}`}>
      <header className={styles.conversationHeader}>
        <Button appearance="standard"
          type="submit"
          variant="ghost"
          className={`${styles.iconButton} ${styles.back}`}
          aria-label="Back to conversations"
          onClick={onBack}
        >
          <ArrowLeft size={20} />
        </Button>
        <div>
          <h2>{room.name}</h2>
          <small>
            {room.kind === "channel"
              ? room.private
                ? "Private channel"
                : "Team channel"
              : room.kind === "group"
                ? "Group conversation"
                : "Direct message"}
          </small>
        </div>
        {room.kind !== "direct" && room.owner_id === userId && (
          <Button appearance="standard"
            type="submit"
            variant="ghost"
            className={styles.iconButton}
            aria-label="Manage conversation"
            onClick={onManage}
          >
            <Settings size={19} />
          </Button>
        )}
      </header>
      {error && (
        <InlineNotice tone="info" role="status" className={styles.error}>
          {error} Reconnecting…
        </InlineNotice>
      )}
      <div
        ref={scroller}
        className={styles.messages}
        role="log"
        aria-label="Messages"
        aria-live="polite"
        onScroll={() => {
          const e = scroller.current;
          if (e) stick.current = e.scrollHeight - e.scrollTop - e.clientHeight < 80;
        }}
      >
        {hasOlder && (
          <Button appearance="standard"
            type="submit"
            variant="ghost"
            className={styles.loadMore}
            onClick={() => void older()}
          >
            Load earlier messages
          </Button>
        )}
        {loading ? (
          <p className={styles.hint}>Loading messages…</p>
        ) : (
          !messages.length && (
            <div className={styles.emptyList}>
              <MessageCircle />
              <p>Say hello to start the conversation.</p>
            </div>
          )
        )}
        {messages.map((message) => (
          <article
            key={message.id}
            className={`${styles.message} ${message.sender_id === userId ? styles.mine : ""}`}
          >
            <div className={styles.messageMeta}>
              <PersonAvatar name={message.sender_name} small />
              <strong>{message.sender_id === userId ? "You" : message.sender_name}</strong>
              <TimeText appearance="standard" value={message.created_at} />
            </div>
            <div className={styles.bubble}>
              {message.body && <p>{message.body}</p>}
              {message.files.map((file) => (
                <a
                  className={styles.file}
                  key={file.id}
                  href={`${root}/files/${file.id}`}
                  target="_blank"
                  rel="noreferrer"
                >
                  {/* Authenticated private image URL; Next image optimization must not cache it. */}
                  {file.media_type.startsWith("image/") ? (
                    <Image
                      src={`${root}/files/${file.id}?variant=preview`}
                      alt={file.original_name}
                      width={320}
                      height={220}
                      unoptimized
                      loading="lazy"
                    />
                  ) : (
                    <Paperclip size={18} />
                  )}
                  <span>
                    {file.original_name}
                    <small>{Math.max(1, Math.ceil(file.bytes / 1024))} KB</small>
                  </span>
                </a>
              ))}
            </div>
          </article>
        ))}
      </div>
      <form className={styles.composer} onSubmit={send}>
        {attachments.length > 0 && (
          <ul className={styles.attachments}>
            {attachments.map((a) => (
              <li key={a.id}>
                {a.file.name}
                <Button appearance="standard"
                  variant="ghost"
                  type="button"
                  disabled={sending}
                  aria-label={`Remove ${a.file.name}`}
                  onClick={() => setAttachments((current) => current.filter((f) => f.id !== a.id))}
                >
                  <X size={14} />
                </Button>
              </li>
            ))}
          </ul>
        )}
        {sendError && (
          <InlineNotice tone="danger" className={styles.error} role="alert">
            {sendError} Your message is still here. Press Send to retry.
          </InlineNotice>
        )}
        <label className={styles.srOnly} htmlFor="chat-message">
          Message
        </label>
        <TextArea appearance="standard"
          id="chat-message"
          value={body}
          disabled={sending}
          maxLength={5000}
          onChange={(e) => setBody(e.target.value)}
          placeholder="Write a message…"
          rows={2}
        />
        <div className={styles.composerFooter}>
          <Field appearance="standard" className={styles.attachButton}>
            <Paperclip size={18} />
            <span>Attach</span>
            <TextInput appearance="standard"
              type="file"
              multiple
              disabled={sending}
              aria-label="Attach files"
              accept=".jpg,.jpeg,.png,.webp,.heic,.heif,.pdf,.csv,image/heic,image/heif"
              onChange={(e) => {
                const selected = Array.from(e.target.files ?? []);
                e.target.value = "";
                if (
                  selected.length + attachments.length > 10 ||
                  selected.some((f) => f.size > 10 * 1024 * 1024 || f.size === 0)
                ) {
                  setSendError("Choose up to 10 files, each between 1 byte and 10 MB.");
                  return;
                }
                setAttachments((current) => [
                  ...current,
                  ...selected.map((file) => ({
                    id: crypto.randomUUID(),
                    file,
                  })),
                ]);
                setSendError("");
              }}
            />
          </Field>
          <small>Up to 10 files · 10 MB each</small>
          <Button appearance="standard"
            clientManaged variant="primary"
            className={styles.primary}
            disabled={sending || (!body.trim() && !attachments.length)}
            type="submit"
          >
            <Send size={16} />
            {sending ? "Sending…" : "Send"}
          </Button>
        </div>
      </form>
    </section>
  );
}

function ConversationEditor({
  root,
  userId,
  editor,
  onClose,
  onSaved,
}: {
  root: string;
  userId: string;
  editor: Editor;
  onClose: () => void;
  onSaved: (id?: string) => Promise<void>;
}) {
  const [kind, setKind] = useState(editor.kind),
    [name, setName] = useState(
      editor.room?.name ?? (editor.editing ? editor.team?.name : "") ?? "",
    ),
    [privateChannel, setPrivate] = useState(editor.room?.private ?? false);
  const [people, setPeople] = useState<MessagingPerson[]>([]),
    [selected, setSelected] = useState<string[]>([]),
    [query, setQuery] = useState(""),
    [error, setError] = useState(""),
    [loading, setLoading] = useState(true),
    [busy, setBusy] = useState(false);
  const version = useRef(editor.room?.version ?? editor.team?.version ?? 1),
    attempt = useRef<{ body: string; key: string } | null>(null);
  useEffect(() => {
    let cancelled = false;
    async function load() {
      try {
        const options = await api<MessagingPerson[]>(
          `${root}/people${editor.kind === "channel" ? `?team=${editor.team?.id}` : ""}`,
        );
        if (cancelled) return;
        setPeople(options);
        if (editor.editing) {
          const detail = await api<{
            members: MessagingPerson[];
            version: number;
          }>(
            `${root}/${editor.kind === "team" ? "teams" : "rooms"}/${editor.room?.id ?? editor.team?.id}/members`,
          );
          if (cancelled) return;
          setSelected(detail.members.filter((p) => p.id !== userId).map((p) => p.id));
          version.current = detail.version;
          setPeople([
            ...options,
            ...detail.members.filter((p) => !options.some((o) => o.id === p.id)),
          ]);
        }
      } catch (e) {
        if (!cancelled) setError((e as Error).message);
      } finally {
        if (!cancelled) setLoading(false);
      }
    }
    void load();
    return () => {
      cancelled = true;
    };
  }, [root, editor, userId]);
  async function search(value: string) {
    setQuery(value);
    // The initial list covers the first 100 accounts; search also reaches larger companies.
    try {
      const options = await api<MessagingPerson[]>(
        `${root}/people?q=${encodeURIComponent(value)}${kind === "channel" ? `&team=${editor.team?.id}` : ""}`,
      );
      setPeople((current) => [
        ...current.filter((p) => selected.includes(p.id)),
        ...options.filter((p) => !selected.includes(p.id)),
      ]);
    } catch (e) {
      setError((e as Error).message);
    }
  }
  async function save(event: FormEvent) {
    event.preventDefault();
    if (busy) return;
    setBusy(true);
    setError("");
    const input = {
      name,
      members: selected,
      kind,
      private: privateChannel,
      ...(editor.editing ? { expectedVersion: version.current } : {}),
    };
    const path = editor.editing
      ? `${kind === "team" ? "teams" : "rooms"}/${editor.room?.id ?? editor.team?.id}`
      : kind === "team"
        ? "teams"
        : kind === "channel"
          ? `teams/${editor.team?.id}/channels`
          : "rooms";
    const body = JSON.stringify({ path, input });
    if (attempt.current?.body !== body) attempt.current = { body, key: crypto.randomUUID() };
    try {
      const result = await post<{ id: string; roomId?: string }>(
        `${root}/${path}`,
        input,
        attempt.current.key,
      );
      await onSaved(kind === "team" ? result.roomId : result.id);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setBusy(false);
    }
  }
  const showMembers = kind !== "channel" || privateChannel;
  return (
    <Dialog appearance="standard"
      open
      onClose={onClose}
      title={`${editor.editing ? "Manage" : kind === "direct" ? "New" : "Create"} ${kind === "direct" ? "chat" : kind}`}
      dismissDisabled={busy}

      className={styles.dialog}

     presentation="form-drawer">
      <form onSubmit={save}>
        {!editor.editing && ["direct", "group"].includes(kind) && (
          <Field appearance="standard">
            Conversation type
            <Select appearance="standard"
              value={kind}
              onChange={(e) => {
                setKind(e.target.value as "direct" | "group");
                setSelected([]);
              }}
            >
              <option value="direct">Direct message</option>
              <option value="group">Group chat</option>
            </Select>
          </Field>
        )}
        {kind !== "direct" && (
          <Field appearance="standard">
            Name
            <TextInput appearance="standard"
              required
              maxLength={120}
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={
                kind === "channel"
                  ? "e.g. Design"
                  : kind === "team"
                    ? "e.g. Product team"
                    : "e.g. Launch planning"
              }
            />
          </Field>
        )}
        {kind === "channel" && !editor.editing && (
          <Field appearance="standard">
            Access
            <Select appearance="standard"
              value={privateChannel ? "private" : "standard"}
              onChange={(e) => setPrivate(e.target.value === "private")}
            >
              <option value="standard">Standard — everyone in the team</option>
              <option value="private">Private — selected team members</option>
            </Select>
          </Field>
        )}
        {showMembers ? (
          <>
            <Field appearance="standard">
              {kind === "direct" ? "Send to" : "Members"}
              <TextInput appearance="standard"
                type="search"
                value={query}
                onChange={(e) => void search(e.target.value)}
                placeholder="Search people by name or email"
              />
            </Field>
            <p className={styles.hint}>
              {kind === "direct"
                ? "Choose someone in your company."
                : "The owner stays a member. New members can see earlier messages."}
            </p>
            <div className={styles.people}>
              {loading ? (
                <p>Loading people…</p>
              ) : (
                people
                  .filter(
                    (p) =>
                      p.id !== userId &&
                      `${p.name} ${p.email}`.toLowerCase().includes(query.toLowerCase()),
                  )
                  .map((person) => (
                    <Field appearance="standard" key={person.id}>
                      <TextInput appearance="standard"
                        type={kind === "direct" ? "radio" : "checkbox"}
                        name="recipient"
                        checked={selected.includes(person.id)}
                        onChange={(e) =>
                          setSelected((current) =>
                            kind === "direct"
                              ? [person.id]
                              : e.target.checked
                                ? [...current, person.id]
                                : current.filter((id) => id !== person.id),
                          )
                        }
                      />
                      <span>
                        <strong>{person.name}</strong>
                        <small>{person.email}</small>
                      </span>
                    </Field>
                  ))
              )}
              {!loading &&
                people.filter(
                  (p) =>
                    p.id !== userId &&
                    `${p.name} ${p.email}`.toLowerCase().includes(query.toLowerCase()),
                ).length === 0 && <p>No matching people.</p>}
            </div>
            <small className={styles.hint}>{selected.length} selected</small>
          </>
        ) : (
          <p className={styles.hint}>
            Everyone in {editor.team?.name} can read and send messages in this channel.
          </p>
        )}
        {error && (
          <InlineNotice tone="danger" className={styles.error} role="alert">
            {error}
          </InlineNotice>
        )}
        <footer>
          <Button appearance="standard" variant="ghost" type="button" onClick={onClose} disabled={busy}>
            Cancel
          </Button>
          <Button appearance="standard"
            clientManaged variant="primary"
            className={styles.primary}
            disabled={
              busy ||
              loading ||
              (kind === "direct" && selected.length !== 1) ||
              (kind === "group" && !selected.length)
            }
            type="submit"
          >
            {busy
              ? "Saving…"
              : editor.editing
                ? "Save changes"
                : kind === "direct"
                  ? "Start chat"
                  : "Create"}
          </Button>
        </footer>
      </form>
    </Dialog>
  );
}
