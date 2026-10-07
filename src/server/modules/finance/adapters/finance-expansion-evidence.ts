import 'server-only';
import type { WorkContext } from '@/server/modules/identity_access/public/authorization';
import type { SqlDatabase } from '@/server/modules/runtime/public/runtime/types';
import { sha256 } from '@/server/modules/runtime/public/crypto';
import { ApiError } from '@/server/modules/runtime/public/http';
import { financeReviewFileSources, financeReviewFileGuards } from '@/server/modules/files/public/finance-review-file-sources';
import { createFinanceReviewFileEvidence } from '../application/review-file-evidence';
function reviewFiles(db: SqlDatabase) { return createFinanceReviewFileEvidence({ error: (...args) => new ApiError(...args), files: (company, target, id) => financeReviewFileSources(db, company, target, id), hash: sha256, now: Date.now }); }
function stageReview(db: SqlDatabase, plan: Awaited<ReturnType<ReturnType<typeof createFinanceReviewFileEvidence>['prepare']>>) { const { actor, targetType, targetId, version, commandId, files, manifest, hash, createdAt } = plan; return { manifest, hash, steps: [...financeReviewFileGuards(db, actor.companyId, targetType, targetId, commandId, files), db.prepare('INSERT INTO finance_expansion_review_files(id,company_id,target_type,target_id,target_version,command_id,manifest_json,manifest_sha256,reviewed_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)').bind(`finance-review-files-${commandId}`, actor.companyId, targetType, targetId, version, commandId, JSON.stringify(manifest), hash, actor.userId, createdAt)] }; }
/** File Sources bind against the owner's version before independent Finance review. */
export async function financeExpansionReviewFiles(db: SqlDatabase, actor: Pick<WorkContext, 'companyId' | 'userId'>, targetType: string, targetId: string, version: number, commandId: string) { return stageReview(db, await reviewFiles(db).prepare(actor, targetType, targetId, version, commandId)); }
export async function financeExpansionReviewFileSteps(db: SqlDatabase, actor: Pick<WorkContext, 'companyId' | 'userId'>, operation: string, recordId: string, version: number, input: unknown, commandId: string) { const plan = await reviewFiles(db).forCommand(actor, operation, recordId, version, input, commandId); return plan ? stageReview(db, plan).steps : []; }
