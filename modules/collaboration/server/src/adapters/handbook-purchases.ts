import 'server-only';
import {createHash} from 'node:crypto';
import type {WorkContext} from '@erpx/identity-access-server/public/authorization';
import {ApiError} from '@erpx/platform-runtime-server/public/http';

/** Pure presentation cache key; retained for the native handbook screens. */
export function handbookPresentationContext(actor:WorkContext){return createHash('sha256').update(JSON.stringify([actor.userId,actor.assignmentId,actor.role,actor.companyId,actor.branchId,actor.departmentId,actor.authority,actor.iam])).digest('hex');}

/**
 * The native handbook HTTP service re-checks Company membership, editor
 * authority and current purchase scope on every read and command. The former
 * TypeScript SQL scope builders and command image witness are removed; the
 * names survive only so historical public barrels keep a valid shape.
 */
const nativeOnly = (): never => {
  throw new ApiError(
    503,
    'NATIVE_HANDBOOK_REQUIRED',
    'Handbook purchase scope is enforced only by the native service.',
  );
};

export const handbookPurchaseAccess = nativeOnly;
export const handbookReadScope = nativeOnly;
export const handbookSourceReadScope = nativeOnly;
export const requireHandbookWorkspaceRead = nativeOnly;
export const withHandbookPurchasedCommand = nativeOnly;
