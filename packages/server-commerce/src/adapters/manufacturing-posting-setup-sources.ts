import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import type {CommercePostingSetupConfiguration} from '@erpx/server-commerce/domain/posting-setup';
import {MFG_POS_LOCK_POSTING_SOURCES_SHARE,COMMERCE_POSTING_MAPPING_STATE_SQL} from './settings-dedupe-sql';
/** The same Commerce→Manufacturing→Inventory→Finance SHARE order used by
 * source readers. No business writer leases are acquired by page discovery. */
export function manufacturingPostingCommerceReadLocks(database:SqlDatabase){return [database.prepare(MFG_POS_LOCK_POSTING_SOURCES_SHARE)];}
export async function manufacturingPostingMappingState(database:SqlDatabase,companyId:string){return database.prepare(COMMERCE_POSTING_MAPPING_STATE_SQL).bind(companyId).first<CommercePostingSetupConfiguration>();}
export {manufacturingPostingDocumentState} from '@erpx/server-commerce/adapters/sales-goods-posting-setup';
