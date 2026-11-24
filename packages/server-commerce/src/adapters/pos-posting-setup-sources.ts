import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {CommercePostingSetupConfiguration} from '@erpx/server-commerce/domain/posting-setup';
import {MFG_POS_LOCK_POSTING_SOURCES_SHARE,COMMERCE_POSTING_MAPPING_STATE_SQL} from './settings-dedupe-sql';
/** The same Commerce→Inventory→POS→Finance SHARE order used by
 * source readers. No business writer leases are acquired by page discovery. */
export function posStartupCommerceReadLocks(database:SqlDatabase){return [database.prepare(MFG_POS_LOCK_POSTING_SOURCES_SHARE)];}
export async function posStartupMappingState(database:SqlDatabase,companyId:string){return database.prepare(COMMERCE_POSTING_MAPPING_STATE_SQL).bind(companyId).first<CommercePostingSetupConfiguration>();}
export {manufacturingPostingDocumentState as posStartupDocumentState} from '@erpx/server-commerce/adapters/sales-goods-posting-setup';
