import 'server-only';
import { financeExpansionEvidence } from '@erpx/collaboration-server/adapters/native-finance-expansion-evidence';
import type { WorkContext } from '@erpx/identity-access-server/public/authorization';
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';
import { sha256 } from '@erpx/platform-runtime-server/public/crypto';
import { ApiError } from '@erpx/platform-runtime-server/public/http';
import { financeReviewFileSources, financeReviewFileGuards } from '@erpx/files-server/public/finance-review-file-sources';
import { createFinanceReviewFileEvidence } from '@erpx/finance-server/application/review-file-evidence';
function reviewFiles(db: SqlDatabase) { return createFinanceReviewFileEvidence({ error: (...args) => new ApiError(...args), files: (company, target, id) => financeReviewFileSources(db, company, target, id), hash: sha256, now: Date.now }); }
async function stageReview(db: SqlDatabase, plan: Awaited<ReturnType<ReturnType<typeof createFinanceReviewFileEvidence>['prepare']>>) { const { actor, targetType, targetId, version, commandId, files, manifest, hash, createdAt } = plan; return { manifest, hash, steps: [...financeReviewFileGuards(db, actor.companyId, targetType, targetId, commandId, files), await financeExpansionEvidence(db, actor.companyId).reviewFile({ id: `finance-review-files-${commandId}`, companyId: actor.companyId, targetType, targetId, targetVersion: version, commandId, manifest, hash, userId: actor.userId, createdAt })] }; }
/** File Sources bind against the owner's version before independent Finance review. */
export async function financeExpansionReviewFiles(db: SqlDatabase, actor: Pick<WorkContext, 'companyId' | 'userId'>, targetType: string, targetId: string, version: number, commandId: string) { return await stageReview(db, await reviewFiles(db).prepare(actor, targetType, targetId, version, commandId)); }
export async function financeExpansionReviewFileSteps(db: SqlDatabase, actor: Pick<WorkContext, 'companyId' | 'userId'>, operation: string, recordId: string, version: number, input: unknown, commandId: string) { const plan = await reviewFiles(db).forCommand(actor, operation, recordId, version, input, commandId); return plan ? (await stageReview(db, plan)).steps : []; }
