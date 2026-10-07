import type { AiEndpoint, AiMessage, AiProviderSnapshot } from '../../agent/public/transport-contract';
import type { BetaEnvironment } from '../../manage_erpx/public/beta-config';
import type { AiResult } from './social-ai';
import type { PostRow } from './social-source';
export type SocialSnapshot = {
    post: PostRow;
    sources: {
        id: string;
        body: string;
        author: string;
        version: number;
    }[];
    hash: string;
    commentIds: string[];
};
export type ResolvedSocialProvider = {
    endpoint: AiEndpoint;
    snapshot: AiProviderSnapshot;
};
export type SocialProviderOutput = {
    result: unknown;
    inputTokens: number;
    outputTokens: number;
};
export type SocialReplyProvider = (endpoint: AiEndpoint, messages: AiMessage[], maxTokens?: number) => Promise<SocialProviderOutput>;
export type SocialDraftProvider = (environment: BetaEnvironment, input: unknown) => Promise<{
    result: AiResult;
    inputTokens: number;
    outputTokens: number;
}>;
export type SocialAiDraft = {
    id: string;
    kind: 'summary' | 'task';
    result: AiResult;
    sourceVersion: number;
    sourceHash: string;
    sources: {
        id: string;
        version: number;
    }[];
    commentIds: string[];
};
