import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import {
  createNativeSalesDocumentsClient,
  createNativeSalesCancellationClient,
  nativeSalesDocumentsAvailable,
  nativeSalesDocumentsConfiguration,
  nativeSalesConfirmedCancellationEligible,
  nativeSalesOrderCancellationAvailable,
  nativeSalesOrderCancellationConfiguration,
  NativeSalesDocumentsError,
  type NativeSalesCancellationReceipt,
  type NativeSalesDocumentsKind,
  type NativeSalesDocumentsPage,
  type NativeSalesDocumentsPageContext,
  type NativeSalesDocumentsReadInput,
} from './native-sales-documents';
import { createNextNativeSalesDocumentsReader } from '../adapters/server/next-native-sales-documents';
import { commerceEntrypointFacts } from '@erpx/server-commerce/public/entrypoint-facts';
import { holdingResources } from '@erpx/server-runtime/public/runtime';

export { nativeSalesDocumentsAvailable, nativeSalesOrderCancellationAvailable };
export type {
  NativeSalesDocumentRow,
  NativeSalesDocumentsKind,
  NativeSalesDocumentsPage,
  NativeSalesDocumentsPageContext,
} from './native-sales-documents';

/** Retained WorkContext render input; the native owner still re-checks scope. */
export function nativeSalesDocumentsPageContext(actor: WorkContext): NativeSalesDocumentsPageContext {
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
    scope: actor.scope,
    authority: actor.authority,
    modules: actor.modules,
    iam: actor.iam,
  };
}

function expected(actor: NativeSalesDocumentsPageContext) {
  const authority = actor.authority;
  if (!authority || !actor.iam)
    throw new NativeSalesDocumentsError('NATIVE_SALES_DOCUMENTS_CONTEXT_CHANGED');
  return {
    companyId: actor.companyId,
    assignmentId: actor.assignmentId,
    revision: authority.revision,
    assignmentVersion: authority.assignmentVersion,
  } as const;
}

function scope(actor: NativeSalesDocumentsPageContext) {
  return {
    branchSlug: actor.scope.kind === 'holding' ? undefined : actor.scope.branchSlug,
    departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : undefined,
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_SALES_DOCUMENTS_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeSalesDocumentsClient(nativeSalesDocumentsConfiguration(raw));
}

export async function nativeSalesDocuments(
  actor: WorkContext,
  kind: NativeSalesDocumentsKind,
  query: { q?: string; status?: string; cursor?: string },
  purchaseHistory?: boolean,
): Promise<NativeSalesDocumentsPage | undefined> {
  const native = client();
  if (!native) return undefined;
  if (purchaseHistory === false)
    throw new ApiError(
      403,
      'PACKAGE_REQUIRED',
      'The current quotation or Sales package history is required.',
    );
  const pageContext = nativeSalesDocumentsPageContext(actor);
  const input: NativeSalesDocumentsReadInput = {
    holdingSlug: actor.holdingSlug,
    kind,
    search: query.q,
    status: query.status,
    cursor: query.cursor,
    ...scope(pageContext),
    expected: expected(pageContext),
  };
  try {
    return await createNextNativeSalesDocumentsReader(
      (incoming, readInput, signal) => native.documents(incoming, pageContext, readInput, signal),
    )(input);
  } catch (error) {
    const failure = nativeSalesDocumentsFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/** Sales-owned native confirmed-cancellation gate for the retained route.
 * Unset configuration or an order outside the confirmed-sale packet returns
 * undefined so the retained TypeScript command remains authoritative. Once the
 * order is native-eligible, a configured native failure is surfaced and never
 * replaced by the retained writer. */
export async function nativeSalesConfirmedCancellation(
  actor: WorkContext,
  id: string,
  expectedVersion: number,
  key: string,
): Promise<NativeSalesCancellationReceipt | undefined> {
  const raw = process.env.ERPX_NATIVE_SALES_CANCELLATION_CONFIG;
  if (raw === undefined) return undefined;
  let client: ReturnType<typeof createNativeSalesCancellationClient>;
  let pageContext: NativeSalesDocumentsPageContext;
  try {
    client = createNativeSalesCancellationClient(
      nativeSalesOrderCancellationConfiguration(raw),
    );
    pageContext = nativeSalesDocumentsPageContext(actor);
  } catch (error) {
    if (error instanceof NativeSalesDocumentsError)
      throw new ApiError(
        503,
        'NATIVE_SALES_UNAVAILABLE',
        'The native Sales service is unavailable.',
      );
    throw error;
  }
  let facts: { kind: string; state: string } | null;
  try {
    const write = (await holdingResources(actor.holdingId)).write;
    facts = await commerceEntrypointFacts(write).orderFacts(id, actor.companyId);
  } catch {
    throw new ApiError(
      503,
      'NATIVE_SALES_UNAVAILABLE',
      'The native Sales service is unavailable.',
    );
  }
  if (!facts || !nativeSalesConfirmedCancellationEligible(facts.kind, facts.state))
    return undefined;
  try {
    return await client.cancel(
      await headers(),
      pageContext,
      id,
      {
        holdingSlug: actor.holdingSlug,
        expectedVersion,
        ...scope(pageContext),
        expected: expected(pageContext),
      },
      key,
    );
  } catch (error) {
    const failure = nativeSalesDocumentsFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

/** Bounded failure policy for the retained page/route error handling. */
export function nativeSalesDocumentsFailure(
  error: unknown,
): { status: number; code: string; message: string } | undefined {
  if (!(error instanceof NativeSalesDocumentsError)) return undefined;
  if (error.code === 'NATIVE_SALES_DOCUMENTS_CONTEXT_CHANGED')
    return {
      status: 409,
      code: 'WORK_CONTEXT_CHANGED',
      message: 'Your work role changed. Reload and review before retrying.',
    };
  if (error.status === 401)
    return { status: 401, code: 'UNAUTHENTICATED', message: 'Sign in to continue.' };
  if (error.status === 403)
    return {
      status: 403,
      code: error.code === 'NATIVE_SALES_DOCUMENTS_UNAVAILABLE' ? 'FORBIDDEN' : error.code,
      message: error.message,
    };
  if (error.status === 404)
    return { status: 404, code: 'NOT_FOUND', message: 'Sales documents are not available in this scope.' };
  if (error.status === 409)
    return { status: 409, code: error.code, message: error.message };
  if (error.status === 422)
    return { status: 422, code: error.code, message: error.message };
  if (error.status === 503)
    return {
      status: 503,
      code: 'NATIVE_SALES_DOCUMENTS_UNAVAILABLE',
      message: 'The native Sales service is unavailable.',
    };
  if (error.code === 'NATIVE_SALES_DOCUMENTS_CONFIGURATION_INVALID')
    return {
      status: 503,
      code: 'NATIVE_SALES_DOCUMENTS_UNAVAILABLE',
      message: 'The native Sales reader is unavailable.',
    };
  if (error.code === 'NATIVE_SALES_DOCUMENTS_REQUEST_INVALID')
    return {
      status: 422,
      code: 'SALES_DOCUMENTS_REQUEST_INVALID',
      message: 'Review the Sales document request.',
    };
  if (
    error.code === 'NATIVE_SALES_DOCUMENTS_AUTHORITY_INVALID' ||
    error.code === 'NATIVE_SALES_DOCUMENTS_COOKIE_LIMIT' ||
    error.code === 'NATIVE_SALES_DOCUMENTS_RESPONSE_INVALID' ||
    error.code === 'NATIVE_SALES_DOCUMENTS_RESPONSE_LIMIT'
  )
    return {
      status: 503,
      code: 'NATIVE_SALES_DOCUMENTS_UNAVAILABLE',
      message: 'The native Sales reader is unavailable.',
    };
  return undefined;
}
