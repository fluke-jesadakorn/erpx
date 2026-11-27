import { createServer, request, type IncomingMessage, type ServerResponse } from 'node:http';
import { PassThrough, type Duplex } from 'node:stream';
import { hasPortListener } from './port-listeners';
import { appIds } from '@erpx/contracts/apps';
import { holdingUiUrl, oasisDemoAccountHost, oasisDemoHolding, parseHoldingRoute } from '@erpx/contracts/holding-hosts';
import { holdingHostForHost } from '@erpx/feature-runtime/lib/holding-hosts';
import { nativeApiError, proxyNativeApi, renderedSelectionHeaders, stripInternalHeaders, workspacePreferencePreconditions } from './native-api-proxy';
import { holdingNativeCutoverPath, isHoldingQualityPath, normalizeHoldingNativePath, oasisDemoNativeRoute, respondToHoldingQualityMethod, respondToShellPreferenceMethod, shellPreferenceRoute, type HoldingNativeWorkspace } from './native-cutover-routes';

export type CompiledGatewayRouting = { schema: 'erpx-compiled-routing/v1'; hosts: Record<string,string>; ports: Record<string,number>; api?: number };
export const oasisDemoRouting = {
  schema: 'erpx-oasis-demo-routing/v1', gateway: 3112, api: 3382,
  ports: { account: 3287, sales: 3288, apps: 3289 },
} as const;
export type OasisDemoRouting = {schema: typeof oasisDemoRouting.schema};
export function isOasisDemoRouting(value: unknown): value is OasisDemoRouting {
  return typeof value === 'object' && value !== null && Object.keys(value).length === 1 &&
    (value as OasisDemoRouting).schema === oasisDemoRouting.schema;
}
/** External release orchestration owns processes. This transport never starts jobs or replays requests. */
export function validateCompiledRouting(value: CompiledGatewayRouting) {
  if (value?.schema !== 'erpx-compiled-routing/v1' || !value.hosts || !value.ports || JSON.stringify(Object.keys(value.ports).sort()) !== JSON.stringify([...appIds].sort())) throw new Error('EXACT_COMPILED_ROUTING_REQUIRED');
  const ports = new Set<number>(), routed = new Set<string>();
  for (const port of Object.values(value.ports)) {
    if (!Number.isInteger(port) || port < 3200 || port > 3299 || ports.has(port)) throw new Error('INVALID_COMPILED_APP_PORT');
    ports.add(port);
  }
  if (value.api !== undefined && (!Number.isInteger(value.api) || value.api < 1024 || value.api > 65535 || [3000,3001,8080].includes(value.api) || ports.has(value.api))) throw new Error('INVALID_COMPILED_API_PORT');
  for (const [host,id] of Object.entries(value.hosts)) {
    if (!/^[a-z0-9](?:[a-z0-9.-]*[a-z0-9])?(?::[0-9]{1,5})?$/.test(host) || new URL('http://'+host).host !== host || !appIds.includes(id as typeof appIds[number])) throw new Error('INVALID_COMPILED_HOST');
    routed.add(id);
  }
  if (routed.size !== appIds.length) throw new Error('EXACT_COMPILED_ROUTING_REQUIRED');
  return { ...value, hosts: Object.freeze({...value.hosts}), ports: Object.freeze({...value.ports}) };
}
function unavailable(response: ServerResponse) {
  if (response.headersSent) {response.destroy();return;}
  response.writeHead(503, {'Cache-Control':'no-store','Content-Type':'text/plain; charset=utf-8'});response.end('Application unavailable.');
}
type OasisTarget = { status: 404 | 405 | 421; allow?: string } |
  { port: number; native: true; path: string; workspace?: HoldingNativeWorkspace; workContext?: true; workspacePreference?: true } | { port: number; native: false; path: string };

export function selectOasisDemoTarget(host: string | undefined, url: string, method: string): OasisTarget {
  if (host !== oasisDemoAccountHost && host !== oasisDemoHolding.hostname) return { status: 421 };
  const account = host === oasisDemoAccountHost;
  const path = url.split('?', 1)[0]!;
  if (!path.startsWith('/') || path.includes('//') || /[\\\u0000-\u0020\u007f]/u.test(path) || /%(?:2f|5c|00)/i.test(path) || /(?:^|\/)(?:\.|%2e){1,2}(?:\/|$)/i.test(path)) return { status: 404 };
  const transport = /^\/__erpx\/(account|sales|apps)(?=\/api(?:\/|$))/.exec(path);
  if (transport && (account !== (transport[1] === 'account'))) return { status: 404 };
  const candidate = transport ? url.slice(transport[0].length) : url;
  const native = oasisDemoNativeRoute(host, candidate);
  if (native) {
    if (!native.methods.includes(method)) return { status: 405, allow: native.methods.join(', ') };
    return { port: oasisDemoRouting.api, native: true, path: native.url, workspace: native.workspace, workContext: native.workContext,
      ...(native.workspacePreference ? {workspacePreference:true} : {}) };
  }
  if (/^\/(?:api|internal|sso)(?:\/|$)/.test(path) || transport || /\/api(?:\/|$)/.test(path) && path.startsWith('/__erpx/')) return { status: 404 };
  if (method !== 'GET' && method !== 'HEAD') return { status: 405, allow: 'GET, HEAD' };
  if (account) return path.startsWith('/__erpx/') ? { status: 404 } : { port: oasisDemoRouting.ports.account, native: false, path: url };
  const route = parseHoldingRoute(oasisDemoHolding.holdingSlug, path, 'oasis-demo-staging', process.env.ERPX_COMPANY_ADDRESS_ROUTING === 'enabled');
  let app = route?.app;
  if (['/launch', '/choose-work-role', '/account/continue'].includes(path)) {
    const navigation = new URL(url, `https://${host}`);
    const declared = navigation.searchParams.get('app');
    const next = navigation.searchParams.get('next');
    if (declared) {
      if (declared !== 'sales' && declared !== 'apps') return { status: 404 };
      app = declared;
    } else if (next) {
      const requested = new URL(next, navigation);
      if (requested.origin === navigation.origin) app = parseHoldingRoute(oasisDemoHolding.holdingSlug, requested.pathname, 'oasis-demo-staging', process.env.ERPX_COMPANY_ADDRESS_ROUTING === 'enabled')?.app;
    }
  }
  if (app !== 'sales' && app !== 'apps') return { status: 404 };
  return { port: oasisDemoRouting.ports[app], native: false, path: url };
}

function oasisNextLocation(host: string, requestPath: string, location: string): string {
  if (host !== oasisDemoHolding.hostname) return location;
  const requestUrl = new URL(requestPath, `https://${host}`), target = new URL(location, requestUrl);
  if (target.origin !== requestUrl.origin) return location;
  return holdingUiUrl(target.href, requestUrl.searchParams.get('company') ?? requestUrl.searchParams.get('companyId') ?? undefined, 'oasis-demo-staging', process.env.ERPX_COMPANY_ADDRESS_ROUTING === 'enabled') ?? location;
}

const oasisRenderedContextHeaders = [...renderedSelectionHeaders, 'x-erpx-workspace-branch', 'x-erpx-workspace-department'];

function oasisCompanySelector(value: string | string[] | undefined) {
  return typeof value === 'string' && /^[a-zA-Z0-9_:-]{1,128}$/.test(value) ? value : undefined;
}

type OasisRoleContext = { valid: boolean; companyId: string; userId: string; assignmentId: string;
  revision: number; assignmentVersion: number; options: {id: string; version: number; modules: string[]}[] };

async function readOasisRoleContext(req: IncomingMessage, workspace: HoldingNativeWorkspace | undefined) {
  const headers = {
    host: oasisDemoHolding.hostname, cookie: req.headers.cookie!, origin: `https://${oasisDemoHolding.hostname}`,
    accept: 'application/json', 'x-erpx-api-transport': 'public',
    ...(workspace?.branch ? {'x-erpx-workspace-branch':workspace.branch} : {}),
    ...(workspace?.department ? {'x-erpx-workspace-department':workspace.department} : {}),
  };
  const selected = oasisCompanySelector(req.headers['x-erpx-requested-company']);
  if (selected) Object.assign(headers, {'x-erpx-context-company': selected});
  const result = await new Promise<{status:number; body:string}>((resolve,reject) => {
    const upstream = request({host:'127.0.0.1', port:oasisDemoRouting.api, method:'GET',
      path:`/api/v1/holdings/${oasisDemoHolding.holdingSlug}/assignments/active`, headers}, response => {
      const chunks: Buffer[] = [];
      let length = 0;
      response.on('data', (chunk: Buffer) => {
        length += chunk.length;
        if (length > 128 * 1024) response.destroy(new Error('OASIS_ROLE_RESPONSE_LIMIT'));
        else chunks.push(chunk);
      });
      response.once('error', reject);
      response.once('end', () => resolve({status:response.statusCode ?? 503,body:Buffer.concat(chunks).toString('utf8')}));
    });
    upstream.setTimeout(5_000, () => upstream.destroy(new Error('OASIS_ROLE_TIMEOUT')));
    upstream.once('error', reject);
    upstream.end();
  });
  if (result.status !== 200) return {result};
  const data = (JSON.parse(result.body) as {data?:OasisRoleContext}).data;
  if (!data?.valid || !data.assignmentId) return {};
  const role = data.options?.find(option => option.id === data.assignmentId && option.version === data.assignmentVersion);
  if (!role || !data.companyId || !data.userId || !Number.isSafeInteger(data.revision) || !Number.isSafeInteger(data.assignmentVersion)) throw new Error('OASIS_ROLE_RESPONSE_INVALID');
  return {context:{'x-erpx-context-company':data.companyId, 'x-erpx-context-user':data.userId,
    'x-erpx-context-assignment':data.assignmentId, 'x-erpx-context-revision':String(data.revision),
    'x-erpx-context-assignment-version':String(data.assignmentVersion), 'x-erpx-context-modules':role.modules.join(','),
    ...(workspace?.branch ? {'x-erpx-workspace-branch':workspace.branch} : {}),
    ...(workspace?.department ? {'x-erpx-workspace-department':workspace.department} : {}),
  } as Record<string, string>};
}

async function proxyOasisDemo(req: IncomingMessage, res: ServerResponse) {
  const host = req.headers.host;
  let target: OasisTarget;
  try { target = selectOasisDemoTarget(host, req.url ?? '/', req.method ?? ''); }
  catch { target = { status: 404 }; }
  if ('status' in target) {
    res.writeHead(target.status, { 'Cache-Control': 'private, no-store',
      ...(target.allow ? {Allow: target.allow} : {}) });
    res.end();
    return;
  }
  const headers = stripInternalHeaders(req.headers);
  if (target.native) {
    headers['x-erpx-api-transport'] = 'public';
    if (target.workspace?.branch) headers['x-erpx-workspace-branch'] = target.workspace.branch;
    if (target.workspace?.department) headers['x-erpx-workspace-department'] = target.workspace.department;
    if (target.workspacePreference) {
      // The owner compares these preconditions against the signed session and
      // the branch/department from the canonical workspace path in the body.
      Object.assign(headers, workspacePreferencePreconditions(req.headers));
    }
    if (target.path.split('?',1)[0]?.endsWith('/assignments/active')) {
      // This is a Company selector. The native IAM owner resolves membership and scope.
      const company = oasisCompanySelector(req.headers['x-erpx-requested-company'] ?? req.headers['x-erpx-context-company']);
      if (company) headers['x-erpx-context-company'] = company;
    }
    if (target.workContext) {
      if (!req.headers.cookie) return nativeApiError(res, 401, 'UNAUTHENTICATED', 'Sign in to continue.');
      let resolved: Awaited<ReturnType<typeof readOasisRoleContext>>;
      try {
        resolved = await readOasisRoleContext(req, target.workspace);
      } catch {
        return nativeApiError(res, 503, 'NATIVE_WORK_CONTEXT_UNAVAILABLE', 'The selected work context is unavailable.');
      }
      const {result, context} = resolved;
      if (result) {
        res.writeHead(result.status, { 'Content-Type':'application/json', 'Cache-Control':'private, no-store' });
        res.end(result.body);
        return;
      }
      if (!context) return nativeApiError(res, 409, 'WORK_ROLE_REQUIRED', 'Choose a work role to continue.');
      if (oasisRenderedContextHeaders.some(name => req.headers[name] !== undefined && req.headers[name] !== context[name]))
        return nativeApiError(res, 409, 'WORK_CONTEXT_CHANGED', 'The work context changed.');
      Object.assign(headers, context);
    }
  } else {
    headers['x-forwarded-host'] = host;
    headers['x-forwarded-proto'] = 'https';
    headers['x-forwarded-port'] = '443';
  }
  if (req.destroyed) return;
  const upstream = request({host:'127.0.0.1', port:target.port, path:target.path, method:req.method, headers}, response => {
    if (!target.native && response.headers.location) response.headers.location = oasisNextLocation(host!, target.path, response.headers.location);
    response.headers['x-robots-tag'] = 'noindex, nofollow';
    res.writeHead(response.statusCode ?? 502, response.statusMessage, response.headers);
    response.pipe(res);
    response.once('error', () => unavailable(res));
  });
  req.once('aborted', () => upstream.destroy());
  res.once('close', () => { if (!res.writableFinished) upstream.destroy(); });
  upstream.once('error', () => unavailable(res));
  req.pipe(upstream);
}

export async function startCompiledWorkspaceGateway(routing: CompiledGatewayRouting | OasisDemoRouting, port: number) {
  const demo = isOasisDemoRouting(routing);
  const config = demo ? null : validateCompiledRouting(routing as CompiledGatewayRouting);
  if (demo && port !== oasisDemoRouting.gateway || config && (!Number.isInteger(port) || port < 1024 || port > 65535 || [3000,3001,8080,...Object.values(config.ports)].includes(port))) throw new Error('EXPLICIT_GATEWAY_PORT_REQUIRED');
  if(hasPortListener(port))throw new Error('GATEWAY_PORT_OCCUPIED');
  const sockets = new Set<Duplex>();
  const selected = (req: IncomingMessage) => {
    if (!config) return undefined;
    const app = config.ports[config.hosts[req.headers.host??'']];
    if (!app) return undefined;
    const path = (req.url ?? '').split('?')[0];
    return config.api !== undefined && (path === '/api' || path.startsWith('/api/')) ? { port: config.api, native: true } : { port: app, native: false };
  };
  const server = createServer((req,res) => {
    if (demo) { void proxyOasisDemo(req,res).catch(() => unavailable(res)); return; }
    const target = selected(req);
    if (!target) {res.writeHead(421,{'Cache-Control':'no-store'});res.end();return;}
    const preference = shellPreferenceRoute(req.url ?? '/');
    if (preference) {
      const app = config!.hosts[req.headers.host ?? ''];
      const holding = holdingHostForHost(req.headers.host);
      const prefixedAllowed = !preference.app || preference.app === app ||
        (holding && preference.app !== 'account' && Object.hasOwn(config!.ports, preference.app));
      if (!prefixedAllowed || app === 'account' && preference.kind === 'workspace') {
        nativeApiError(res, 404, 'NOT_FOUND', 'Not found.');
        return;
      }
      if (respondToShellPreferenceMethod(res, req.method, preference.kind, !holding && (app === 'pms' || app === 'pos'))) return;
      if (!config!.api) {
        nativeApiError(res, 503, 'NATIVE_PREFERENCES_UNAVAILABLE', 'The native preference service is unavailable.');
        return;
      }
      void proxyNativeApi({base:new URL(`http://127.0.0.1:${config!.api}`), req, res, url:preference.url, shellPreference:preference.kind})
        .catch(() => unavailable(res));
      return;
    }
    if (target.native) {
      const url = req.url ?? '/';
      const holdingSlug = /^\/api\/v1\/holdings\/([a-z0-9-]+)\//.exec(url)?.[1];
      const scoped = holdingSlug === undefined ? undefined : normalizeHoldingNativePath(url, holdingSlug);
      if (holdingSlug && scoped && isHoldingQualityPath(scoped.url, holdingSlug) && respondToHoldingQualityMethod(res, req.method)) return;
      const cutover = holdingSlug && scoped && holdingNativeCutoverPath(scoped.url, holdingSlug, req.method);
      if (cutover === 'routine-editor-private') {
        res.writeHead(404, {'Cache-Control':'private, no-store'});
        res.end();
        return;
      }
      void proxyNativeApi({ base: new URL(`http://127.0.0.1:${target.port}`), environment: process.env, req, res,
        contextGuard: cutover === 'social' || cutover === 'plugins' || cutover === 'navigation-command' || cutover === 'quotation-summary' ? cutover : undefined,
        url: cutover && scoped ? scoped.url : url, workspace: cutover && scoped ? scoped.workspace : undefined,
        requiresSession: (cutover === 'routine-templates' || cutover === 'routine-schedules') && ['GET', 'HEAD'].includes(req.method ?? ''),
        recordAction: cutover === 'record-action', routineCommand: cutover === 'routine-command', qualityRead: cutover === 'quality-read',
        qualityCommand: cutover === 'quality-command', qualityMissing: cutover === 'quality-missing' }).catch(() => unavailable(res));
      return;
    }
    // Host/Origin, cookie headers, path/query and multipart/RSC/SSE bodies pass unchanged.
    const upstream=request({host:'127.0.0.1',port:target.port,path:req.url,method:req.method,headers:req.headers},response=>{
      res.writeHead(response.statusCode??502,response.statusMessage,response.headers);response.pipe(res);
      response.once('error',()=>unavailable(res));
    });
    req.once('aborted',()=>upstream.destroy());upstream.once('error',()=>unavailable(res));
    res.once('close',()=>{if(!res.writableFinished)upstream.destroy();});req.pipe(upstream);
  });
  server.on('upgrade',(req,downstream,head)=>{
    if (demo) { downstream.end('HTTP/1.1 404 Not Found\r\nConnection: close\r\nContent-Length: 0\r\n\r\n'); return; }
    const target=selected(req);
    if(!target){downstream.end('HTTP/1.1 421 Misdirected Request\r\nConnection: close\r\nContent-Length: 0\r\n\r\n');return;}
    const waiting=new PassThrough({highWaterMark:64*1024});
    downstream.once('error',()=>downstream.destroy());downstream.once('end',()=>downstream.destroy());
    downstream.once('close',()=>waiting.destroy());waiting.once('error',()=>downstream.destroy());downstream.pipe(waiting);
    const upstream=request({host:'127.0.0.1',port:target.port,path:req.url,method:req.method,headers:target.native?stripInternalHeaders(req.headers):req.headers});
    downstream.once('close',()=>upstream.destroy());
    upstream.once('upgrade',(response,socket,upstreamHead)=>{
      if(downstream.destroyed){socket.destroy();return;}
      sockets.add(downstream);downstream.once('close',()=>{sockets.delete(downstream);socket.destroy();});socket.once('close',()=>downstream.destroy());
      downstream.write(`HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\n`+response.rawHeaders.reduce((text,value,index,values)=>index%2?text:text+value+': '+values[index+1]+'\r\n','')+'\r\n');
      if(upstreamHead.length)downstream.write(upstreamHead);if(head.length)socket.write(head);
      socket.pipe(downstream);waiting.pipe(socket);socket.once('error',()=>downstream.destroy());downstream.once('error',()=>socket.destroy());
    });
    upstream.once('response',response=>{downstream.write(`HTTP/1.1 ${response.statusCode} ${response.statusMessage}\r\nConnection: close\r\n\r\n`);response.pipe(downstream);response.once('error',()=>downstream.destroy());});
    upstream.once('error',()=>downstream.destroy());upstream.end();
  });
  await new Promise<void>((done,reject)=>{server.once('error',reject);server.listen(port,'127.0.0.1',()=>{server.off('error',reject);done();});});
  return {server,close:async()=>{for(const socket of sockets)socket.end();await new Promise<void>((done,reject)=>server.close(error=>error?reject(error):done()));}};
}
