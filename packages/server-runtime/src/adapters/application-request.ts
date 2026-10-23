import 'server-only';

/** Normalize only this server's explicit loopback transport address. Public
 * authority comes from composition configuration and the unchanged Host header;
 * client forwarded headers never choose the destination. */
export function applicationRequest(request: Request, origin: string | null, port: string | undefined): Request {
  if (!origin || !port || !/^32\d{2}$/.test(port) || request.method !== 'GET') return request;
  const expected=new URL(origin),source=new URL(request.url);
  if(request.headers.get('host')!==expected.host||source.hostname!=='127.0.0.1'||source.port!==port
    ||!['http:','https:'].includes(source.protocol)||source.username||source.password||source.hash)return request;
  return new Request(new URL(source.pathname+source.search,expected),request);
}
