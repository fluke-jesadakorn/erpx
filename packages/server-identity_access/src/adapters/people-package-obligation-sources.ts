import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
/** Lifecycle-owned review identity only. Standing Organization grants and
 * independent IT preparation do not belong to the People product. */
export const peoplePackageReviewSources="SELECT id,company_id,status,version,reviewed_by FROM iam_reviews";
export function peoplePackageReviewSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE iam_reviews IN SHARE MODE")];}
