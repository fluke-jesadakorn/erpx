export interface StreamChatMessage {
  role: 'system' | 'user' | 'assistant';
  content: string;
}

export interface StreamChatRequest {
  messages: StreamChatMessage[];
  sectionKey?: string;
  sessionId?: string;
  model?: string;
  thinking?: 'auto' | 'low' | 'medium' | 'high';
  lang?: 'en' | 'th' | 'de';
  scope?: { tileId?: string; displayName?: string; hint?: string };
  editMessageId?: string;
}

export interface StreamChatBlocks {
  charts?: unknown[];
  htmls?: string[];
  sqls?: unknown[];
}

export interface StreamChatMeta {
  sessionId: string;
  userMessageId?: string;
  assistantMessageId?: string;
  modelName: string;
  latencyMs: number;
  blocks?: StreamChatBlocks;
}

export interface StreamChatHandlers {
  onChunk: (chunk: string) => void;
  onMeta?: (meta: StreamChatMeta) => void;
  onError?: (err: Error) => void;
  signal?: AbortSignal;
}

interface SsePayload {
  chunk?: string;
  delta?: string;
  meta?: StreamChatMeta;
  sessionId?: string;
  userMessageId?: string;
  assistantMessageId?: string;
  modelName?: string;
  latencyMs?: number;
  blocks?: StreamChatBlocks;
  error?: string;
  message?: string;
  done?: boolean;
}

export async function streamChat(req: StreamChatRequest, h: StreamChatHandlers): Promise<void> {
  const { onChunk, onMeta, onError, signal } = h;
  let response: Response;
  try {
    response = await fetch('/api/ai/chat/full', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(req),
      signal,
    });
  } catch (e) {
    onError?.(e instanceof Error ? e : new Error(String(e)));
    return;
  }

  if (!response.ok || !response.body) {
    const msg = `HTTP ${response.status}`;
    onError?.(new Error(msg));
    return;
  }

  const reader = response.body.getReader();
  const decoder = new TextDecoder('utf-8');
  let buffer = '';

  const onAbort = () => {
    reader.cancel().catch(() => {});
  };
  signal?.addEventListener('abort', onAbort, { once: true });

  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      let idx: number;
      while ((idx = buffer.indexOf('\n\n')) >= 0) {
        const rawEvent = buffer.slice(0, idx);
        buffer = buffer.slice(idx + 2);
        const line = rawEvent.split('\n').find((l) => l.startsWith('data:'));
        if (!line) continue;
        const data = line.slice(5).trim();
        if (!data) continue;
        let payload: SsePayload;
        try {
          payload = JSON.parse(data) as SsePayload;
        } catch {
          continue;
        }
        const chunkText = payload.chunk ?? payload.delta;
        if (typeof chunkText === 'string' && chunkText.length > 0) {
          onChunk(chunkText);
        }
        const errMsg = payload.error ?? payload.message;
        if (typeof errMsg === 'string' && errMsg.length > 0) {
          onError?.(new Error(errMsg));
        }
        const metaObj: StreamChatMeta | undefined =
          payload.meta ??
          (payload.sessionId
            ? {
                sessionId: payload.sessionId,
                userMessageId: typeof payload.userMessageId === 'string' ? payload.userMessageId : undefined,
                assistantMessageId: typeof payload.assistantMessageId === 'string' ? payload.assistantMessageId : undefined,
                modelName: payload.modelName ?? '',
                latencyMs: payload.latencyMs ?? 0,
                blocks: payload.blocks,
              }
            : undefined);
        if (metaObj && onMeta) {
          onMeta(metaObj);
        }
        if (payload.done) {
          return;
        }
      }
    }
  } catch (e) {
    if (signal?.aborted) return;
    onError?.(e instanceof Error ? e : new Error(String(e)));
  } finally {
    signal?.removeEventListener('abort', onAbort);
    try { reader.releaseLock(); } catch { /* already released */ }
  }
}
