import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import {
  createNativeAgentEvidenceClient,
  nativeAgentEvidenceConfiguration,
  nativeAgentEvidenceFailure,
  nativeAgentEvidencePageContext,
  NativeAgentEvidenceError,
  type NativeAgentEvidence,
} from './native-agent-evidence';

function client() {
  const raw = process.env.ERPX_NATIVE_AGENT_CONFIG;
  if (raw === undefined) return undefined;
  return createNativeAgentEvidenceClient(nativeAgentEvidenceConfiguration(raw));
}

export function nativeAgentEvidenceAvailable(): boolean {
  return process.env.ERPX_NATIVE_AGENT_CONFIG !== undefined;
}

/**
 * Native Agent evidence read. When the exact private configuration is absent
 * the caller must render the evidence as unavailable; there is no retained
 * TypeScript invocation/session read on this path once the section is cut over.
 */
export async function nativeAgentEvidenceFromHeaders(
  actor: WorkContext,
  holdingSlug: string,
  incoming: Pick<Headers, 'get'>,
  signal?: AbortSignal,
): Promise<NativeAgentEvidence | undefined> {
  const instance = client();
  if (!instance) return undefined;
  if (actor.holdingSlug !== holdingSlug)
    throw new NativeAgentEvidenceError('NATIVE_AGENT_CONTEXT_CHANGED');
  const context = nativeAgentEvidencePageContext(actor);
  try {
    return await instance.evidence(incoming, context, signal);
  } catch (error) {
    if (error instanceof NativeAgentEvidenceError) throw error;
    throw new NativeAgentEvidenceError('NATIVE_AGENT_UNAVAILABLE');
  }
}

export { nativeAgentEvidenceFailure, NativeAgentEvidenceError };
export type {
  NativeAgentEvidence,
  NativeAgentInvocation,
  NativeAgentPageContext,
  NativeAgentSession,
} from './native-agent-evidence';
