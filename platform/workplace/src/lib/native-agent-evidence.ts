import { headers } from 'next/headers';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import type { NativeAgentEvidence } from '@erpx/agent-server/public/native-agent-evidence';
import {
  nativeAgentEvidenceFromHeaders,
  NativeAgentEvidenceError,
} from '@erpx/agent-server/public/native-agent-evidence-page';

export { nativeAgentEvidenceAvailable, nativeAgentEvidenceFailure } from '@erpx/agent-server/public/native-agent-evidence-page';

export async function nativeAgentEvidence(
  actor: WorkContext,
  holdingSlug: string,
): Promise<NativeAgentEvidence | undefined> {
  if (actor.holdingSlug !== holdingSlug) throw new NativeAgentEvidenceError('NATIVE_AGENT_CONTEXT_CHANGED');
  return nativeAgentEvidenceFromHeaders(actor, holdingSlug, await headers());
}
