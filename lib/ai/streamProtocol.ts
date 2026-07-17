import 'server-only';
// Shared helpers for streaming AI responses from route handlers and reading them
// from client islands. All SSE streams in /api/ai/* use the same envelope:
//
//   data: { "chunk": "..." }\n\n          (zero or more)
//   event: meta\ndata: {...}\n\n          (once, at end)
//   event: error\ndata: {...}\n\n        (on failure)

export type StreamChunk = { chunk: string };
export type StreamMeta = {
  modelName?: string;
  latencyMs?: number;
  usedModel?: string;
  fellBack?: boolean;
  blocks?: { plain?: string; charts?: unknown[]; htmls?: string[]; sqls?: unknown[]; uis?: Array<{ id?: string; root?: unknown }> };
};
export type StreamError = { message: string; statusCode?: number; upstreamCode?: string | null; upstreamMessage?: string | null };

export interface SseEnvelope<T = unknown> {
  event?: string;
  data: T;
}

export async function* toSseStream(
  src: AsyncGenerator<string, void, void>,
  metaAtEnd?: StreamMeta
): AsyncGenerator<string, void, void> {
  for await (const chunk of src) {
    yield `data: ${JSON.stringify({ chunk })}\n\n`;
  }
  if (metaAtEnd) {
    yield `event: meta\ndata: ${JSON.stringify(metaAtEnd)}\n\n`;
  }
}

export function sseHeaders(): Record<string, string> {
  return {
    'Content-Type': 'text/event-stream; charset=utf-8',
    'Cache-Control': 'no-cache, no-transform',
    'Connection': 'keep-alive',
    'X-Accel-Buffering': 'no',
  };
}

export function sseError(err: StreamError): string {
  return `event: error\ndata: ${JSON.stringify(err)}\n\n`;
}

export interface ParsedSseMessage {
  event?: string;
  data: unknown;
}

export class SseReader {
  private buffer = '';
  constructor(private reader: ReadableStreamDefaultReader<Uint8Array>) {}

  async *[Symbol.asyncIterator](): AsyncGenerator<string, StreamMeta, void> {
    const dec = new TextDecoder();
    while (true) {
      const { value, done } = await this.reader.read();
      if (done) return undefined as unknown as StreamMeta;
      this.buffer += dec.decode(value, { stream: true });
      let idx: number;
      while ((idx = this.buffer.indexOf('\n\n')) >= 0) {
        const frame = this.buffer.slice(0, idx);
        this.buffer = this.buffer.slice(idx + 2);
        const parsed = parseFrame(frame);
        if (!parsed) continue;
        if (parsed.event === 'meta') return parsed.data as StreamMeta;
        if (parsed.event === 'error') {
          const msg = (parsed.data as StreamError)?.message || 'AI stream error';
          throw new Error(msg);
        }
        if (typeof (parsed.data as StreamChunk).chunk === 'string') {
          yield (parsed.data as StreamChunk).chunk;
        }
      }
    }
  }
}

function parseFrame(frame: string): ParsedSseMessage | null {
  let event: string | undefined;
  const dataLines: string[] = [];
  for (const line of frame.split('\n')) {
    if (line.startsWith('event:')) event = line.slice(6).trim();
    else if (line.startsWith('data:')) dataLines.push(line.slice(5).trim());
  }
  if (dataLines.length === 0) return null;
  const raw = dataLines.join('\n');
  if (raw === '[DONE]') return null;
  try {
    return { event, data: JSON.parse(raw) };
  } catch {
    return null;
  }
}

export async function openSseStream(
  url: string,
  body: unknown,
  signal?: AbortSignal
): Promise<{ iterator: AsyncGenerator<string, { modelName?: string; latencyMs?: number; fellBack?: boolean; blocks?: StreamMeta['blocks'] }, void>; close: () => void }> {
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
    signal,
  });
  if (!res.ok || !res.body) {
    const t = await res.text().catch(() => '');
    throw new Error(`HTTP ${res.status}${t ? ': ' + t.slice(0, 200) : ''}`);
  }
  const reader = res.body.getReader();
  const r = new SseReader(reader);
  const it = r[Symbol.asyncIterator]() as AsyncGenerator<string, { modelName?: string; latencyMs?: number; fellBack?: boolean; blocks?: StreamMeta['blocks'] }, void>;
  return { iterator: it, close: () => reader.cancel().catch(() => {}) };
}
