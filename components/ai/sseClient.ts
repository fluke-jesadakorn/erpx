'use client';

export type StreamMeta = { modelName?: string; latencyMs?: number; fellBack?: boolean; blocks?: any; [k: string]: unknown };

export interface SseStreamHandle {
  iterator: AsyncGenerator<string, StreamMeta, void>;
  close: () => void;
}

export async function openSseStream(
  url: string,
  body: unknown,
  signal?: AbortSignal
): Promise<SseStreamHandle> {
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
  const dec = new TextDecoder();
  let buffer = '';
  const queue: { event?: string; data: any }[] = [];
  let pendingResolve: (() => void) | null = null;
  let streamError: Error | null = null;

  function push() {
    if (pendingResolve) {
      const r = pendingResolve;
      pendingResolve = null;
      r();
    }
  }

  (async () => {
    try {
      while (true) {
        const { value, done } = await reader.read();
        if (done) break;
        buffer += dec.decode(value, { stream: true });
        let idx: number;
        while ((idx = buffer.indexOf('\n\n')) >= 0) {
          const frame = buffer.slice(0, idx);
          buffer = buffer.slice(idx + 2);
          let event: string | undefined;
          const dataLines: string[] = [];
          for (const line of frame.split('\n')) {
            if (line.startsWith('event:')) event = line.slice(6).trim();
            else if (line.startsWith('data:')) dataLines.push(line.slice(5).trim());
          }
          if (dataLines.length === 0) continue;
          const raw = dataLines.join('\n');
          if (raw === '[DONE]') continue;
          let parsed: any;
          try { parsed = JSON.parse(raw); } catch { continue; }
          if (event === 'meta') {
            queue.push({ event, data: parsed });
          } else if (event === 'error') {
            streamError = new Error(parsed?.message || 'AI stream error');
            queue.push({ event, data: parsed });
          } else if (parsed && typeof parsed.chunk === 'string') {
            queue.push({ event, data: parsed });
          } else if (parsed && typeof parsed.delta === 'string') {
            queue.push({ event, data: parsed });
          }
          push();
        }
      }
    } finally {
      push();
    }
  })();

  const iterator = (async function* () {
    while (true) {
      if (streamError) throw streamError;
      if (queue.length > 0) {
        const next = queue.shift()!;
        if (next.event === 'meta') return next.data as StreamMeta;
        if (next.event === 'error') throw new Error((next.data as any)?.message || 'AI stream error');
        const payload = next.data as any;
        const text = typeof payload.chunk === 'string' ? payload.chunk : payload.delta;
        if (typeof text === 'string') yield text;
      } else {
        await new Promise<void>((resolve) => {
          pendingResolve = resolve;
        });
      }
    }
  })();

  return {
    iterator,
    close: () => reader.cancel().catch(() => {}),
  };
}
