import 'server-only';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { dateSchema } from '@erpx/legal-server/public/rules/model';
import { z } from 'zod';
import {
  createNativeLegalClient,
  nativeLegalConfiguration,
  NativeLegalError,
  nativeLegalFailure,
  type NativeLegalCommandResult,
  type NativeLegalCitation,
  type NativeLegalDetail,
  type NativeLegalExport,
  type NativeLegalPageContext,
  type NativeLegalRegister,
} from '../adapters/server/native-legal';

/** Call only after the native citation request has failed and authorized its actor. */
export function nativeLegalCitationPageError(error: unknown, id: string, asOf: string | string[] | undefined): unknown {
  if (error instanceof NativeLegalError && error.status === 422 && error.code === 'LEGAL_CITATION_VALIDATION' && id.startsWith('law:')) {
    // These are the same schemas and order as the legacy law application.
    // Parsing only after the owner response preserves authorization precedence.
    if (asOf !== undefined) dateSchema.parse(asOf);
    z.string().min(1).max(200).parse(id);
    return error; // The marker was inconsistent: retain an error boundary.
  }
  const failure = nativeLegalFailure(error);
  return failure ? new ApiError(failure.status, failure.code, failure.message) : error;
}

type Query = Record<string, string | string[] | undefined>;

function context(actor: WorkContext): NativeLegalPageContext {
  return {
    userId: actor.userId,
    holdingId: actor.holdingId,
    holdingSlug: actor.holdingSlug,
    companyId: actor.companyId,
    branchId: actor.branchId,
    departmentId: actor.departmentId,
    assignmentId: actor.assignmentId,
    role: actor.role,
    assignmentScope: actor.assignmentScope,
    scope:
      actor.scope.kind === 'holding'
        ? { kind: 'holding' }
        : actor.scope.kind === 'branch'
          ? { kind: 'branch', branchSlug: actor.scope.branchSlug }
          : {
              kind: 'department',
              branchSlug: actor.scope.branchSlug,
              departmentSlug: actor.scope.departmentSlug,
            },
    authority: actor.authority,
    modules: actor.modules,
    iam: actor.iam,
  };
}

function client(actor: WorkContext) {
  const raw = process.env.ERPX_NATIVE_LEGAL_RECORDS_CONFIG;
  if (raw === undefined) return undefined;
  if (!actor.authority || !actor.iam)
    throw new NativeLegalError('NATIVE_LEGAL_CONTEXT_CHANGED');
  return createNativeLegalClient(nativeLegalConfiguration(raw));
}

/** Full native citation payload; absence of the private binding fails closed. */
export async function nativeLegalCitation(actor: WorkContext, holdingSlug: string, id: string, asOf: string | string[] | undefined, incoming: Pick<Headers, 'get'>): Promise<NativeLegalCitation> {
  const native = client(actor);
  if (!native || actor.holdingSlug !== holdingSlug) throw new NativeLegalError('NATIVE_LEGAL_UNAVAILABLE');
  return native.citation(incoming, context(actor), id, asOf);
}

/** Selector presence only; unset configuration keeps the explicit retained
 * fallback without probing the private service. */
export function nativeLegalAvailable(): boolean {
  return process.env.ERPX_NATIVE_LEGAL_RECORDS_CONFIG !== undefined;
}

function search(query: Query): URLSearchParams {
  const out = new URLSearchParams();
  const kind = query.kind;
  if (kind !== undefined) {
    if (typeof kind !== 'string' || kind.length === 0)
      throw new NativeLegalError('NATIVE_LEGAL_REQUEST_INVALID');
    out.set('kind', kind);
  }
  const search = query.search;
  if (search !== undefined) {
    if (typeof search !== 'string' || search.length > 200)
      throw new NativeLegalError('NATIVE_LEGAL_REQUEST_INVALID');
    if (search) out.set('search', search);
  }
  const state = query.state;
  if (state !== undefined) {
    if (typeof state !== 'string' || state.length > 40)
      throw new NativeLegalError('NATIVE_LEGAL_REQUEST_INVALID');
    if (state) out.set('state', state);
  }
  const page = query.page;
  if (page !== undefined) {
    if (typeof page !== 'string' || !/^[1-9][0-9]{0,5}$/.test(page))
      throw new NativeLegalError('NATIVE_LEGAL_REQUEST_INVALID');
    out.set('page', page);
  }
  const mine = query.mine;
  if (mine !== undefined) {
    if (typeof mine !== 'string' || !['true', 'false'].includes(mine))
      throw new NativeLegalError('NATIVE_LEGAL_REQUEST_INVALID');
    out.set('mine', mine);
  }
  return out;
}

/** Native register read. A missing configuration keeps the retained page/route;
 * a configured failure is mapped by the caller instead of silently falling back. */
export async function nativeLegalRegister(
  actor: WorkContext,
  holdingSlug: string,
  query: Query,
  incoming: Pick<Headers, 'get'>,
): Promise<NativeLegalRegister | undefined> {
  const native = client(actor);
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeLegalError('NATIVE_LEGAL_CONTEXT_CHANGED');
  return native.register(incoming, context(actor), search(query));
}

/** Native record detail, including documents, holds, shares and access history. */
export async function nativeLegalDetail(
  actor: WorkContext,
  holdingSlug: string,
  id: string,
  incoming: Pick<Headers, 'get'>,
): Promise<NativeLegalDetail | undefined> {
  const native = client(actor);
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeLegalError('NATIVE_LEGAL_CONTEXT_CHANGED');
  return native.detail(incoming, context(actor), id);
}

/** Native export access. The Files owner still serves the immutable original
 * bytes; the Legal owner records the export access event. */
export async function nativeLegalExport(
  actor: WorkContext,
  holdingSlug: string,
  id: string,
  format: string | null,
  incoming: Pick<Headers, 'get'>,
): Promise<NativeLegalExport | undefined> {
  const native = client(actor);
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeLegalError('NATIVE_LEGAL_CONTEXT_CHANGED');
  return native.export(incoming, context(actor), id, format);
}

/** Bounded document/hold/archive/member/share actions are selected natively.
 * Other Legal commands remain on the retained owner until their native slice lands. */
const COMMANDS = new Set([
  'document',
  'hold.create',
  'hold.request-release',
  'hold.release',
  'transition',
  'share',
  'share.revoke',
  'member',
  'executed.review',
  'approval.open',
  'approval.decide',
  'approval.reroute',
  'executed.import',
  'document.import',
  'document.inspect',
  'assign',
  'comment',
  'link',
  'hold.acknowledge',
  'approve',
]);

export function nativeLegalCommandSelected(body: unknown): boolean {
  if (!body || typeof body !== 'object' || Array.isArray(body)) return false;
  const action = (body as Record<string, unknown>).action;
  if (typeof action !== 'string' || !COMMANDS.has(action)) return false;
  return true;
}

export async function nativeLegalCommand(
  actor: WorkContext,
  holdingSlug: string,
  body: unknown,
  idempotencyKey: string,
  incoming: Pick<Headers, 'get'>,
): Promise<NativeLegalCommandResult | undefined> {
  const native = client(actor);
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeLegalError('NATIVE_LEGAL_CONTEXT_CHANGED');
  if (!nativeLegalCommandSelected(body))
    throw new NativeLegalError('NATIVE_LEGAL_REQUEST_INVALID');
  return native.command(incoming, context(actor), body, idempotencyKey);
}

export { nativeLegalFailure } from '../adapters/server/native-legal';
