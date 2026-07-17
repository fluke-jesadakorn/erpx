import 'server-only';
import { invokeStream, type InvokeInput, type InvokeStreamInput, type InvokeResult } from './router';
import { sseHeaders, sseError } from './streamProtocol';

export interface StreamRequestCtx {
  actorId: number;
  sectionKey: string;
  task: 'embed' | 'chat' | 'vision';
  input: Omit<InvokeStreamInput, 'modelOverride' | 'actorId'> & { modelOverride?: string };
}

export async function streamSse(
  req: StreamRequestCtx,
  onComplete?: (text: string, providerId: number | undefined, modelId: number | undefined, modelName: string | undefined, latencyMs: number) => void | Promise<void>,
  onError?: (err: { message: string; statusCode?: number; upstreamCode?: string | null; upstreamMessage?: string | null }) => void
): Promise<Response> {
  const t0 = Date.now();
  const enc = new TextEncoder();
  let fullText = '';
  let providerId: number | undefined;
  let modelId: number | undefined;
  let modelName: string | undefined;
  try {
    const it = invokeStream(req.sectionKey, req.task, { ...req.input, actorId: req.actorId }, { actorId: req.actorId });
    const stream = new ReadableStream({
      async start(controller) {
        try {
          for await (const chunk of it) {
            fullText += chunk;
            controller.enqueue(enc.encode(`data: ${JSON.stringify({ chunk })}\n\n`));
          }
          const latencyMs = Date.now() - t0;
          controller.enqueue(enc.encode(`event: meta\ndata: ${JSON.stringify({ modelName: modelName ?? null, latencyMs, usedModel: modelName ?? null })}\n\n`));
          if (onComplete) await onComplete(fullText, providerId, modelId, modelName, latencyMs);
          controller.close();
        } catch (e: any) {
          const err = {
            message: e?.message || 'AI stream failed',
            statusCode: e?.statusCode ?? e?.response?.status ?? undefined,
            upstreamCode: e?.upstreamCode ?? e?.response?.data?.error?.code ?? null,
            upstreamMessage: e?.upstreamMessage ?? e?.response?.data?.error?.message ?? null,
          };
          controller.enqueue(enc.encode(sseError(err)));
          if (onError) onError(err);
          controller.close();
        }
      },
    });
    return new Response(stream, { headers: sseHeaders() });
  } catch (e: any) {
    const err = {
      message: e?.message || 'AI stream failed',
      statusCode: e?.statusCode ?? undefined,
      upstreamCode: e?.upstreamCode ?? null,
      upstreamMessage: e?.upstreamMessage ?? null,
    };
    return new Response(sseError(err), { headers: { 'Content-Type': 'text/event-stream' }, status: 200 });
  }
}

export interface InvokeOnceRequest {
  actorId: number;
  sectionKey: string;
  task: 'embed' | 'chat' | 'vision';
  input: Omit<InvokeInput, 'actorId'> & { modelOverride?: string };
}

export async function invokeOnce(req: InvokeOnceRequest): Promise<InvokeResult> {
  const r = await import('./router').then(m => m.invoke);
  return r(req.sectionKey, req.task, { ...req.input, actorId: req.actorId }, { actorId: req.actorId });
}
