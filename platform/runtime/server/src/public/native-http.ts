import 'server-only';
import { Agent, request } from 'node:http';
import { Readable } from 'node:stream';

/**
 * Minimal Node HTTP transport for private loopback calls that must carry an
 * explicit `Host` header. Node's fetch forbids overriding `Host`, so every
 * native transport that selects a registered Holding uses this helper instead.
 * Callers keep their own configuration, validation and fail-closed policy.
 */
export function nativeRequest(
  url: URL,
  init: Readonly<{
    method?: string;
    headers: Record<string, string>;
    body?: string | Uint8Array;
    signal?: AbortSignal;
  }>,
  timeoutMs: number,
): Promise<Response> {
  return new Promise((resolve, reject) => {
    const agent = new Agent({
      keepAlive: false,
      maxSockets: 1,
      maxTotalSockets: 1,
      proxyEnv: {} as import('node:http').ProxyEnv,
    });
    const body = init.body;
    const headers: Record<string, string> = { ...init.headers };
    if (body !== undefined) headers['content-length'] = String(typeof body === 'string' ? new TextEncoder().encode(body).length : body.byteLength);
    const outgoing = request(
      url,
      {
        method: init.method ?? 'GET',
        agent,
        headers,
        signal: init.signal ?? AbortSignal.timeout(timeoutMs),
        maxHeaderSize: 8192,
      },
      incoming => {
        incoming.once('close', () => agent.destroy());
        try {
          const values = new Headers();
          for (let i = 0; i < incoming.rawHeaders.length; i += 2)
            values.append(incoming.rawHeaders[i]!, incoming.rawHeaders[i + 1]!);
          resolve(
            new Response(Readable.toWeb(incoming) as unknown as ReadableStream<Uint8Array>, {
              status: incoming.statusCode ?? 502,
              headers: values,
            }),
          );
        } catch (error) {
          incoming.destroy();
          agent.destroy();
          reject(error);
        }
      },
    );
    outgoing.once('error', error => {
      agent.destroy();
      reject(error);
    });
    if (body !== undefined) outgoing.write(body);
    outgoing.end();
  });
}
