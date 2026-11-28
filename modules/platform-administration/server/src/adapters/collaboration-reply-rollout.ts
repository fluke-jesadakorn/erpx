import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {releaseAcceptanceRowSql} from '@erpx/platform-administration-server/adapters/release-predicate';
/** The owner-native reply-guard read owns this statement. The synchronous guard
 *  statement is rendered from the shared owner-local row head so the caller's
 *  batch stays byte-identical and no inline statement remains in this adapter. */
export function socialReplyEngineeringLock(db:SqlDatabase,company:string){return db.prepare(releaseAcceptanceRowSql('company_id','company_id=?',' FOR SHARE','engineering_verification_contexts')).bind(company);}
