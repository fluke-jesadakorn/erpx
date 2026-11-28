import 'server-only';
import { headers } from 'next/headers';
import {
  createNativeBrandingClient,
  nativeBrandingFailure,
  type NativeBrandingCommandInput,
  type NativeBrandingReadInput,
} from './native-branding';
import type { NativeBrandIdentityPageContext } from './native-brand-identity';
import { NativeBrandIdentityError } from './native-brand-identity';

function expected(actor: NativeBrandIdentityPageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam) throw new NativeBrandIdentityError('NATIVE_BRANDING_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeBrandIdentityPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_BRANDING_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeBrandingClient(raw);
}

/** Native-only Branding reader. No TypeScript writer fallback. */
export async function nativeBrandingWorkspace(
  actor: NativeBrandIdentityPageContext,
  holdingSlug: string,
): Promise<Record<string, unknown> | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug) throw new NativeBrandIdentityError('NATIVE_BRANDING_CONTEXT_CHANGED');
  return native.read(await headers(), actor, {
    holdingSlug,
    ...scope(actor),
    expected: expected(actor),
  } satisfies NativeBrandingReadInput);
}

/** Native-only Branding command. No TypeScript writer fallback. */
export async function nativeBrandingCommand(
  actor: NativeBrandIdentityPageContext,
  holdingSlug: string,
  body: unknown,
  idempotencyKey: string,
): Promise<Record<string, unknown> | undefined> {
  const native = client();
  if (!native) return undefined;
  if (actor.holdingSlug !== holdingSlug) throw new NativeBrandIdentityError('NATIVE_BRANDING_CONTEXT_CHANGED');
  const incoming = await headers();
  return native.command(incoming, actor, {
    holdingSlug,
    ...scope(actor),
    expected: expected(actor),
    body,
    idempotencyKey,
    origin: incoming.get('origin') ?? undefined,
  } satisfies NativeBrandingCommandInput);
}

export { nativeBrandingFailure };
export type { NativeBrandingCommandInput, NativeBrandingReadInput };
