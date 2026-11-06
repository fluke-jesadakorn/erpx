import 'server-only';
import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import { ApiError } from '@erpx/server-runtime/public/http';
import { holdingResources } from '@erpx/server-runtime/public/runtime';
import { commerceEntrypointFacts } from '@erpx/server-commerce/public/entrypoint-facts';
import {
  createNativeCommerceOrdersClient,
  nativeCommerceOrdersAvailable,
  nativeCommerceOrdersConfiguration,
  nativeCommerceOrdersFailure,
  NativeCommerceOrdersError,
  type NativeCommerceOrderDocument,
  type NativeCommerceOrderReceipt,
  type NativeCommerceOrderTransitionInput,
  type NativeCommerceOrdersPageContext,
  type NativeCommerceOrdersReadInput,
} from '@erpx/feature-commerce/public/native-commerce-orders';
import { nativeSalesDocumentTransitionEligible } from './native-sales-documents';

export { nativeCommerceOrdersAvailable };
export type {
  NativeCommerceOrderDocument,
  NativeCommerceOrderLineRow,
  NativeCommerceOrderReceipt,
  NativeCommerceOrderRow,
  NativeCommerceOrderTransitionInput,
} from '@erpx/feature-commerce/public/native-commerce-orders';

export function nativeCommerceOrdersPageContext(
  actor: WorkContext,
): NativeCommerceOrdersPageContext {
  if (!actor.authority || !actor.iam)
    throw new NativeCommerceOrdersError('NATIVE_COMMERCE_ORDER_CONTEXT_CHANGED');
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

function expected(context: NativeCommerceOrdersPageContext) {
  if (!context.authority || !context.iam)
    throw new NativeCommerceOrdersError('NATIVE_COMMERCE_ORDER_CONTEXT_CHANGED');
  return {
    companyId: context.companyId,
    assignmentId: context.assignmentId,
    revision: context.authority.revision,
    assignmentVersion: context.authority.assignmentVersion,
  } as const;
}

function scope(context: NativeCommerceOrdersPageContext) {
  return {
    branchSlug: context.scope.kind === 'holding' ? undefined : context.scope.branchSlug,
    departmentSlug:
      context.scope.kind === 'department' ? context.scope.departmentSlug : undefined,
  };
}

function readInput(context: NativeCommerceOrdersPageContext): NativeCommerceOrdersReadInput {
  return {
    holdingSlug: context.holdingSlug,
    ...scope(context),
    expected: expected(context),
  };
}

function client() {
  const raw = process.env.ERPX_NATIVE_COMMERCE_ORDER_CONFIG;
  if (raw === undefined) return undefined;
  try {
    return createNativeCommerceOrdersClient(nativeCommerceOrdersConfiguration(raw));
  } catch (error) {
    const failure = nativeCommerceOrdersFailure(error);
    if (failure) throw new ApiError(failure.status, failure.code, failure.message);
    throw error;
  }
}

function fail(error: unknown): never {
  const failure = nativeCommerceOrdersFailure(error);
  if (failure) throw new ApiError(failure.status, failure.code, failure.message);
  throw error;
}

/**
 * Native Commerce order document read. An unset configuration retains the
 * TypeScript reader; once configured, an invalid or failed native read is
 * surfaced and never silently replaced.
 */
export async function nativeCommerceOrderPage(
  actor: WorkContext,
  id: string,
): Promise<NativeCommerceOrderDocument | undefined> {
  const native = client();
  if (!native) return undefined;
  try {
    const context = nativeCommerceOrdersPageContext(actor);
    return await native.detail(await headers(), context, id, readInput(context));
  } catch (error) {
    return fail(error);
  }
}

/**
 * Native transition gate for the retained route. Unset configuration or an
 * order/action outside this bounded slice returns undefined so the retained
 * path remains authoritative. Once an order/action is native-eligible, a
 * configured native failure is surfaced and never replaced by the retained
 * command.
 */
export async function nativeCommerceOrderTransition(
  actor: WorkContext,
  id: string,
  input: NativeCommerceOrderTransitionInput,
  key: string,
): Promise<NativeCommerceOrderReceipt | undefined> {
  const native = client();
  if (!native) return undefined;
  let context: NativeCommerceOrdersPageContext;
  let facts: { kind: string; state: string } | null;
  try {
    context = nativeCommerceOrdersPageContext(actor);
    const write = (await holdingResources(actor.holdingId)).write;
    facts = await commerceEntrypointFacts(write).orderFacts(id, actor.companyId);
  } catch {
    // The preflight is part of the configured native path; a failure must not
    // fall back to the retained command.
    throw new ApiError(
      503,
      'NATIVE_COMMERCE_ORDER_UNAVAILABLE',
      'The native Commerce order service is unavailable.',
    );
  }
  if (!facts) return undefined;
  if (!nativeSalesDocumentTransitionEligible(facts.kind, facts.state, input.action))
    return undefined;
  try {
    return await native.transition(await headers(), context, id, input, key, readInput(context));
  } catch (error) {
    return fail(error);
  }
}
