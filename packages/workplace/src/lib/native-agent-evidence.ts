import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/server-identity_access/public/authorization';
import type { NativeAgentEvidence } from '@erpx/server-agent/public/native-agent-evidence';
import {
  nativeAgentEvidenceFromHeaders,
  NativeAgentEvidenceError,
} from '@erpx/server-agent/public/native-agent-evidence-page';

export { nativeAgentEvidenceAvailable, nativeAgentEvidenceFailure } from '@erpx/server-agent/public/native-agent-evidence-page';

export async function nativeAgentEvidence(
  actor: WorkContext,
  holdingSlug: string,
): Promise<NativeAgentEvidence | undefined> {
  if (actor.holdingSlug !== holdingSlug) throw new NativeAgentEvidenceError('NATIVE_AGENT_CONTEXT_CHANGED');
  return nativeAgentEvidenceFromHeaders(actor, holdingSlug, await headers());
}
