import {holdingHostForHost,holdingRoutingEnabled,parseHoldingRoute,holdingUiUrl,holdingFrontUiUrl} from '@erpx/contracts/holding-hosts';
import {requestProductOrigin,ssoAudienceForHost} from '@erpx/feature-runtime/lib/product-hosts';
import 'server-only';
import { NextResponse } from 'next/server';
import { z } from 'zod';
import { productIds, isProductHostKey } from '@erpx/contracts/product-registry';
import { configuredProductOrigin, configuredProductHandoff, productHostForHost } from '@erpx/feature-runtime/lib/product-hosts';
import { product } from '@erpx/server-commercial/public/product-registry';
import { currentSession } from '@erpx/server-identity_access/public/authorization';
import { providerSwitcherStore } from '@erpx/server-identity_access/public/provider-switcher';
import { productWorkspaceTarget, productWorkRoleChoices, selectProductWorkRole, productPageAccess } from '@erpx/server-identity_access/public/product-workspace';
import { adminWorkspaceTarget, adminWorkRoleChoices, selectAdminWorkRole, adminPageAccess } from '@erpx/server-identity_access/public/admin-workspace';
import { safeProductReturnPath } from '@erpx/server-identity_access/public/rules/product-sso';
import { randomBytes } from 'node:crypto';
import { frontRoleState } from '@erpx/server-work_projection/adapters/front-app';
import { selectWorkRole } from '@erpx/server-work_projection/adapters/work-role-store';
import { productLaunchChoices } from '@erpx/server-work_projection/adapters/product-launcher';
import { adminLaunchChoices } from '@erpx/server-work_projection/adapters/admin-launcher';
import { ApiError, errorResponse } from '@erpx/server-runtime/public/http';
import type { SwitcherContext, SwitcherWorkspace } from '@erpx/feature-identity_access/lib/device-accounts';
import { withCurrentWorkSelectionSession } from '@erpx/server-identity_access/public/private-session-read';
import { providerContextSelection } from '@erpx/feature-identity_access/lib/provider-context';

const responseHeaders = { 'Cache-Control': 'private, no-store', 'Referrer-Policy': 'no-referrer' };
const inputSchema = z.object({ workspaceKey: z.string().min(1).max(4096), assignmentId: z.string().min(1).max(128), revision: z.number().int().nonnegative(), version: z.union([z.number().int().positive(), z.string().regex(/^[a-f0-9]{64}$/)]), accountKey: z.string().min(1).max(128) }).strict();
const denied = (error: unknown) => error instanceof ApiError && [403,404].includes(error.status);
function target(href: string) {
  const outer = new URL(href), next = outer.searchParams.get('next') ?? '/';
  const inner = new URL(next, outer.origin); return inner.pathname === '/launch' || inner.pathname === '/admin/launch' ? inner.searchParams.get('next') ?? '/' : inner.pathname + inner.search;
}
async function workspaces(): Promise<SwitcherWorkspace[]> {
  const [products, admin] = await Promise.all([productLaunchChoices(), adminLaunchChoices()]);
  return [
    ...products.map(choice => ({ key: choice.href, label: `${choice.holding} · ${choice.company}${choice.workspace !== choice.company ? ` · ${choice.workspace}` : ''} · ${product(choice.productId)!.label.en}`,
      kind: (product(choice.productId)?.shell === 'front' && !target(choice.href).startsWith('/holdings/') ? 'front' : 'work') as 'front'|'work', productId: choice.productId, companyId: choice.companyId, target: target(choice.href) })),
    ...admin.map(choice => ({ key: choice.href, label: `${choice.holding} · ${choice.company} · ${choice.workspace} · ${choice.kind === 'management' ? 'ERPX Management' : 'Administration'}`, kind: 'admin' as const, productId: 'admin', companyId: choice.companyId, target: target(choice.href) })),
  ];
}
function frontParts(workspace: SwitcherWorkspace) {
  const parts = new URL(workspace.target, 'https://front.invalid').pathname.split('/').filter(Boolean).map(decodeURIComponent);
  if (!['pos','pms'].includes(workspace.productId) || parts.length < 3) throw new ApiError(422, 'INVALID_FRONT_WORKSPACE', 'Select an authorized daily workspace.');
  return { holding: parts[0], app: workspace.productId as 'pos'|'pms', location: parts.slice(2).join('/') };
}
async function roles(workspace: SwitcherWorkspace) {
  if (workspace.kind === 'front') {
    const parts = frontParts(workspace), state = await frontRoleState(parts.holding, workspace.companyId, parts.app, parts.location);
    return { roles: state.options.map(option => ({ id: option.id, label: option.label ?? option.role.replaceAll('_',' '), version: option.version!, selected: state.selected?.id === option.id })), revision: Number(state.selection?.revision ?? 0) };
  }
  const input = workspace.kind === 'admin' ? adminWorkspaceTarget(workspace.companyId, workspace.target) : isProductHostKey(workspace.productId) ? productWorkspaceTarget(workspace.productId, workspace.companyId, workspace.target) : null;
  if (!input) throw new ApiError(422, 'INVALID_WORKSPACE', 'Select an authorized workspace.');
  const state = workspace.kind === 'admin' ? await adminWorkRoleChoices(adminWorkspaceTarget(workspace.companyId, workspace.target)!) : await productWorkRoleChoices(workspace.productId as Parameters<typeof productWorkRoleChoices>[0], input);
  return { roles: state.options.map(option => ({ id: option.assignmentId, label: option.label, version: option.assignmentVersion, selected: state.selectedId === option.assignmentId })), revision: Number(state.revision) };
}
async function inspect(request: Request, workspaceKey?: string | null): Promise<SwitcherContext> {
  const { session } = await currentSession(), choices = await workspaces();
  const workspace = choices.find(choice => choice.key === workspaceKey) ?? null;
  const state = workspace ? await roles(workspace) : { roles: [], revision: 0 };
  let provider: SwitcherContext['provider'] = null;
  try { provider = await (await providerSwitcherStore(request)).read(); } catch (error) { if (!denied(error)) throw error; }
  return { user: { id: session.user.id, name: session.user.name, email: session.user.email }, workspaces: choices, workspace, ...state, provider };
}
function authorizedHost(request: Request) {
  const audience = productHostForHost(request.headers.get('host'));
  if (!audience || !['apps','admin',...productIds].includes(audience)) throw new ApiError(404,'NOT_FOUND','Not found.');
  return audience;
}
export async function switcherContextRequest(request: Request) {
  try {
    authorizedHost(request);
    // Cheap focus check does not initialize roles or enumerate every workspace.
    if (new URL(request.url).searchParams.get('check') === '1') { const { session } = await currentSession(); return NextResponse.json({ data: { user: { id: session.user.id, name: session.user.name, email: session.user.email } } }, { headers: responseHeaders }); }
    return NextResponse.json({ data: await inspect(request, new URL(request.url).searchParams.get('workspace')) }, { headers: responseHeaders });
  } catch (error) { return errorResponse(error); }
}
export async function roleSelectionRequest(request: Request) {
  try {
    const audience = authorizedHost(request);
    if (request.headers.get('origin') !== requestProductOrigin(audience,request.headers)) throw new ApiError(403, 'ORIGIN_FORBIDDEN', 'Select a role in this application.');
    if (!request.headers.get('content-type')?.startsWith('application/json')) throw new ApiError(415,'JSON_REQUIRED','Submit the displayed role choice.');
    const body = await request.text(); if (Buffer.byteLength(body) > 8192) throw new ApiError(413,'REQUEST_TOO_LARGE','This request is too large.');
    let raw:unknown;try{raw=JSON.parse(body);}catch{throw new ApiError(422,'VALIDATION_FAILED','Select a current assigned role.');}
    const parsed = inputSchema.safeParse(raw); if (!parsed.success) throw new ApiError(422,'VALIDATION_FAILED','Select a current assigned role.');
    const input = parsed.data, { session } = await currentSession();
    if (input.accountKey !== session.user.id) throw new ApiError(409,'ACCOUNT_CHANGED','Your account changed. Review the current roles.');
    if (input.workspaceKey === 'provider') {
      const store = await providerSwitcherStore(request), before = await store.read();
      if (before.revision !== input.revision || before.choices.find(choice => choice.id === input.assignmentId)?.version !== input.version) throw new ApiError(409,'PROVIDER_CONTEXT_CHANGED','Review the current Platform roles.');
      const key = request.headers.get('Idempotency-Key'); if (!key || !/^[A-Za-z0-9_.:-]{8,128}$/.test(key)) throw new ApiError(422,'IDEMPOTENCY_KEY_REQUIRED','Use a captured role request key.');
      await store.select(providerContextSelection(before, input.assignmentId), key);
      return NextResponse.json({ data: { href: audience === 'admin' ? '/provider' : configuredProductHandoff('admin','/provider') } }, { headers: responseHeaders });
    }
    const workspace = (await workspaces()).find(choice => choice.key === input.workspaceKey);
    if (!workspace || typeof input.version !== 'number') throw new ApiError(409,'WORK_CONTEXT_CHANGED','Your workspace access changed. Review the current choices.');
    await withCurrentWorkSelectionSession({ userId: session.user.id, sessionId: session.session.id, email: session.user.email, audience: ssoAudienceForHost(request.headers.get('host')) }, async assertCurrent => {
      await assertCurrent();
      if (workspace.kind === 'front') {
        const parts = frontParts(workspace), state = await frontRoleState(parts.holding, workspace.companyId, parts.app, parts.location);
        const option = state.options.find(option => option.id === input.assignmentId && option.version === input.version);
        if (!option) throw new ApiError(409,'WORK_CONTEXT_CHANGED','Review the current daily roles.');
        await selectWorkRole(state.write, { userId: state.access.userId, sessionId: state.access.sessionId, companyId: workspace.companyId, assignmentId: option.id, assignmentVersion: option.version!, revision: input.revision, expiresAt: state.access.expiresAt });
      } else if (workspace.kind === 'admin') await selectAdminWorkRole(adminWorkspaceTarget(workspace.companyId, workspace.target)!, input.assignmentId, input.revision, input.version as number);
      else await selectProductWorkRole(workspace.productId as Parameters<typeof selectProductWorkRole>[0], productWorkspaceTarget(workspace.productId as Parameters<typeof selectProductWorkRole>[0], workspace.companyId, workspace.target)!, input.assignmentId, input.revision, input.version as number);
      await assertCurrent();
    });
    // Same-host navigation keeps the selected session. Cross-host entry restores
    // the existing per-account Company role preference through normal SSO.
    const href = audience === workspace.productId ? workspace.target : configuredProductHandoff(workspace.productId as Parameters<typeof configuredProductHandoff>[0], workspace.target);
    return NextResponse.json({ data: { href } }, { headers: responseHeaders });
  } catch (error) { return errorResponse(error); }
}
/** Clean SSO continuation. Workspace hints never substitute owner admission. */
export async function accountContinueRequest(request: Request) {
  const audience = authorizedHost(request), url = new URL(request.url);
  let from = safeProductReturnPath(url.searchParams.get('next'));
  const holding=holdingRoutingEnabled(process.env)&&holdingHostForHost(request.headers.get('host'));
  if(holding)from=parseHoldingRoute(holding.holdingSlug,new URL(from,url).pathname)?.internalPath??from;
  const { session } = await currentSession();
  let destination = configuredProductHandoff('apps')!;
  try {
    if (audience === 'apps' || from === '/') destination = '/';
    else if (audience === 'admin' && /^\/provider(?:\/|$)/.test(from)) { await (await providerSwitcherStore(request)).read(); destination = from; }
    else {
      const choices = await workspaces(), companyId = url.searchParams.get('companyId');
      const workspace = choices.find(choice => choice.productId === audience && (!companyId || choice.companyId === companyId) &&
        (choice.kind === 'front' ? from.startsWith(choice.target.replace(/\/$/, '') + '/') || from === choice.target :
          new URL(from,'https://return.invalid').pathname.startsWith(choice.target.slice(0, choice.target.lastIndexOf('/')) + '/')));
      if (workspace?.kind === 'front') destination = from;
      else if (workspace) {
        const state = workspace.kind === 'admin' ? await adminPageAccess(workspace.companyId, from) : await productPageAccess(audience, workspace.companyId, from);
        destination = state.target ?? state.chooseRole;
      }
    }
  } catch (error) { if (!denied(error) && !(error instanceof ApiError && error.status === 422)) throw error; }
  if (url.searchParams.get('resume') === '1') {
    const nonce = randomBytes(18).toString('base64'), ownOrigin = requestProductOrigin(audience,request.headers)!;
    const ok = session.user.id === url.searchParams.get('account') && destination !== configuredProductHandoff('apps');
    return new Response(`<!doctype html><meta charset="utf-8"><script nonce="${nonce}">parent.postMessage(${JSON.stringify({type:'erpx:account-resumed',ok,userId:session.user.id}).replaceAll('<','\\u003c')},${JSON.stringify(ownOrigin)})</script>`, {headers:{...responseHeaders,'Content-Type':'text/html; charset=utf-8','Content-Security-Policy':`default-src 'none'; script-src 'nonce-${nonce}'; frame-ancestors ${ownOrigin}`}});
  }
  if(holding)destination=holdingUiUrl(destination,url.searchParams.get('companyId')??undefined)??(audience==='pos'||audience==='pms'?holdingFrontUiUrl(destination,audience):null)??destination;
  return NextResponse.redirect(new URL(destination, requestProductOrigin(audience,request.headers)!), {status:303,headers:responseHeaders});
}
