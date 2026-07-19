'use client';
import { useCallback, useEffect, useRef, useState } from 'react';
import { streamChat } from '@/chat/streaming';
import type { ChatSession, ChatMessage, ChatBlocks, SqlResolved } from '@/chat/history';
import type { ChartSpec } from '@/components/chat/chartContract';
import { DEFAULT_CHAT_MODEL, type ChatThinkingLevel } from '@/ai/defaults';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';

const SS_KEY = 'folio.chat.global.sessionId';
const MODEL_KEY = 'folio.chat.global.model';
const THINK_KEY = 'folio.chat.global.thinking';

export interface UseChatSessionInput {
  sectionKey?: string;
  scope?: { tileId: string; displayName: string; hint?: string };
}

export interface UseChatSession {
  sessionId: string | null;
  sessions: ChatSession[];
  messages: ChatMessage[];
  model: string;
  setModel: (m: string) => void;
  thinking: ChatThinkingLevel;
  setThinking: (t: ChatThinkingLevel) => void;
  pending: boolean;
  streamingContent: string;
  streamingBlocks: { charts: ChartSpec[]; htmls: string[]; sqls: SqlResolved[] };
  send: (text: string) => Promise<void>;
  editMessage: (messageId: string, text: string) => Promise<void>;
  newSession: () => Promise<void>;
  switchSession: (id: string) => Promise<void>;
  deleteSession: (id: string) => Promise<void>;
  refreshSessions: () => Promise<void>;
  abort: () => void;
}

export function useChatSession(input: UseChatSessionInput = {}): UseChatSession {
  const locale = useSecondaryLocale();
  const [sessionId, setSessionId] = useState<string | null>(null);
  const [sessions, setSessions] = useState<ChatSession[]>([]);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [model, setModelState] = useState<string>('');
  const [thinking, setThinkingState] = useState<ChatThinkingLevel>('auto');
  const [pending, setPending] = useState(false);
  const [streamingContent, setStreamingContent] = useState('');
  const [streamingBlocks, setStreamingBlocks] = useState({ charts: [] as ChartSpec[], htmls: [] as string[], sqls: [] as SqlResolved[] });
  const abortRef = useRef<AbortController | null>(null);
  const contentRef = useRef('');

  useEffect(() => {
    try {
      const sid = localStorage.getItem(SS_KEY);
      if (sid) setSessionId(sid);
      const m = localStorage.getItem(MODEL_KEY);
      if (m) setModelState(m);
      const t = localStorage.getItem(THINK_KEY);
      if (t === 'auto' || t === 'low' || t === 'medium' || t === 'high') setThinkingState(t);
    } catch {}
  }, []);

  const setModel = useCallback((m: string) => {
    setModelState(m);
    try { localStorage.setItem(MODEL_KEY, m); } catch {}
  }, []);
  const setThinking = useCallback((t: ChatThinkingLevel) => {
    setThinkingState(t);
    try { localStorage.setItem(THINK_KEY, t); } catch {}
  }, []);

  const appendContent = useCallback((delta: string) => {
    contentRef.current += delta;
    setStreamingContent(contentRef.current);
  }, []);
  const resetContent = useCallback(() => {
    contentRef.current = '';
    setStreamingContent('');
  }, []);

  const refreshSessions = useCallback(async () => {
    try {
      const r = await fetch('/api/ai/chat/full/sessions');
      const d = await r.json();
      if (Array.isArray(d?.sessions)) setSessions(d.sessions as ChatSession[]);
    } catch {}
  }, []);

  const ensureSession = useCallback(async (): Promise<string> => {
    if (sessionId) return sessionId;
    try {
      const existing = localStorage.getItem(SS_KEY);
      if (existing) {
        setSessionId(existing);
        return existing;
      }
    } catch {}
    const r = await fetch('/api/ai/chat/full/sessions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ model: model || DEFAULT_CHAT_MODEL }),
    });
    const d = await r.json();
    const sid = d?.session?.id;
    if (sid) {
      setSessionId(sid);
      try { localStorage.setItem(SS_KEY, sid); } catch {}
      await refreshSessions();
    }
    return sid ?? '';
  }, [sessionId, model, refreshSessions]);

  useEffect(() => { void refreshSessions(); }, [refreshSessions]);

  useEffect(() => {
    if (!sessionId) { setMessages([]); return; }
    let cancel = false;
    fetch(`/api/ai/chat/full/sessions/${sessionId}`)
      .then((r) => {
        if (!r.ok) throw new Error(`HTTP ${r.status}`);
        return r.json();
      })
      .then((d) => {
        if (cancel) return;
        if (Array.isArray(d?.messages)) setMessages(d.messages as ChatMessage[]);
      })
      .catch(() => {
        if (cancel) return;
        try { localStorage.removeItem(SS_KEY); } catch {}
        setSessionId(null);
        setMessages([]);
      });
    return () => { cancel = true; };
  }, [sessionId]);

  const run = useCallback(async (text: string, editMessageId?: string) => {
    const content = text.trim();
    if (!content || pending) return;
    const sid = await ensureSession();
    if (!sid) return;
    const checkpoint = editMessageId
      ? messages.findIndex((message) => message.id === editMessageId && message.role === 'user')
      : -1;
    if (editMessageId && checkpoint < 0) return;
    const prior = checkpoint >= 0 ? messages.slice(0, checkpoint) : messages;
    const tempId = `t${Date.now()}`;
    const userMsg: ChatMessage = {
      id: tempId,
      sessionId: sid,
      role: 'user',
      content,
      blocks: { plain: content, charts: [], htmls: [], sqls: [] },
      createdAt: new Date().toISOString(),
    };
    setMessages([...prior, userMsg]);
    setPending(true);
    resetContent();
    setStreamingBlocks({ charts: [], htmls: [], sqls: [] });
    const ctrl = new AbortController();
    abortRef.current = ctrl;
    try {
      const history = prior.map((message) => ({ role: message.role, content: message.content }));
      await streamChat(
        {
          messages: [...history, { role: 'user' as const, content }],
          sessionId: sid,
          sectionKey: input.sectionKey ?? 'chat:global',
          model,
          thinking,
          lang: locale,
          scope: input.scope,
          editMessageId,
        },
        {
          signal: ctrl.signal,
          onChunk: appendContent,
          onMeta: (meta) => {
            if (meta.sessionId && meta.sessionId !== sid) {
              setSessionId(meta.sessionId);
              try { localStorage.setItem(SS_KEY, meta.sessionId); } catch {}
            }
            if (meta.blocks) {
              setStreamingBlocks({
                charts: (meta.blocks.charts ?? []) as ChartSpec[],
                htmls: (meta.blocks.htmls ?? []) as string[],
                sqls: (meta.blocks.sqls ?? []) as SqlResolved[],
              });
            }
            const aiMsg: ChatMessage = {
              id: meta.assistantMessageId ?? `t${Date.now() + 1}`,
              sessionId: meta.sessionId ?? sid,
              role: 'assistant',
              content: contentRef.current,
              blocks: (meta.blocks as unknown as ChatBlocks) ?? {
                plain: contentRef.current,
                charts: [],
                htmls: [],
                sqls: [],
              },
              modelName: meta.modelName ?? null,
              latencyMs: meta.latencyMs ?? null,
              createdAt: new Date().toISOString(),
            };
            setMessages((current) => {
              const next = current.map((message) => message.id === tempId && meta.userMessageId
                ? { ...message, id: meta.userMessageId }
                : message);
              return [...next, aiMsg];
            });
            resetContent();
            void refreshSessions();
          },
          onError: (e) => {
            const errMsg: ChatMessage = {
              id: `t${Date.now()}`,
              sessionId: sid,
              role: 'system',
              content: `⚠️ ${e.message}`,
              blocks: { plain: '', charts: [], htmls: [], sqls: [] },
              createdAt: new Date().toISOString(),
            };
            setMessages((m) => [...m, errMsg]);
          },
        },
      );
    } finally {
      setPending(false);
      abortRef.current = null;
    }
  }, [pending, messages, model, thinking, locale, ensureSession, appendContent, resetContent, refreshSessions, input.scope, input.sectionKey]);

  const send = useCallback((text: string) => run(text), [run]);
  const editMessage = useCallback((messageId: string, text: string) => run(text, messageId), [run]);

  const newSession = useCallback(async () => {
    try { localStorage.removeItem(SS_KEY); } catch {}
    setSessionId(null);
    setMessages([]);
    const r = await fetch('/api/ai/chat/full/sessions', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ model: model || DEFAULT_CHAT_MODEL }),
    });
    const d = await r.json();
    if (d?.session?.id) {
      setSessionId(d.session.id);
      try { localStorage.setItem(SS_KEY, d.session.id); } catch {}
      await refreshSessions();
    }
  }, [model, refreshSessions]);

  const switchSession = useCallback(async (id: string) => {
    setSessionId(id);
    try { localStorage.setItem(SS_KEY, id); } catch {}
  }, []);

  const deleteSession = useCallback(async (id: string) => {
    try {
      await fetch(`/api/ai/chat/full/sessions/${id}`, { method: 'DELETE' });
    } catch {}
    setSessions((s) => s.filter((x) => x.id !== id));
    if (sessionId === id) {
      try { localStorage.removeItem(SS_KEY); } catch {}
      setSessionId(null);
      setMessages([]);
    }
  }, [sessionId]);

  const abort = useCallback(() => { abortRef.current?.abort(); }, []);

  return {
    sessionId, sessions, messages,
    model, setModel,
    thinking, setThinking,
    pending, streamingContent, streamingBlocks,
    send, editMessage, newSession, switchSession, deleteSession, refreshSessions, abort,
  };
}
