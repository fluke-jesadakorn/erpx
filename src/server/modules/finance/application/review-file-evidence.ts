import { createFinanceReviewFilePolicy, type FinanceReviewActor } from '../domain/review-file-evidence';
import type { FinanceError } from '../domain/invoice-line-revision';
import type { FinanceReviewFileSource } from '@/server/modules/files/public/finance-review-contract';
export interface FinanceReviewFilePorts {
    error: FinanceError;
    files(companyId: string, targetType: string, targetId: string): Promise<FinanceReviewFileSource[]>;
    hash(bytes: string): Promise<string>;
    now(): number;
}
export function createFinanceReviewFileEvidence(ports: FinanceReviewFilePorts) {
    const policy = createFinanceReviewFilePolicy(ports.error);
    async function prepare(actor: FinanceReviewActor, targetType: string, targetId: string, version: number, commandId: string) { const files = await ports.files(actor.companyId, targetType, targetId), manifest = policy.manifest(files), hash = await ports.hash(JSON.stringify(manifest)); return { actor, targetType, targetId, version, commandId, files, manifest, hash, createdAt: ports.now() }; }
    async function forCommand(actor: FinanceReviewActor, operation: string, recordId: string, version: number, input: unknown, commandId: string) { const review = policy.reviewedTarget(operation, version, input); return review ? prepare(actor, review.target, recordId, review.expected, commandId) : null; }
    return { prepare, forCommand };
}
