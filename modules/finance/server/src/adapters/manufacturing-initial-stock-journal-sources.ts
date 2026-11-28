import 'server-only';
import { JOURNAL_SELECT_PREFIX } from './finance-read-tail-r30-sql';
import {rootOwnerExistsSql} from './finance-read-tail-r19-sql';
import {bookRootUnionSql} from './finance-read-tail-r23-sql';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {inventoryFinanceRecordSources} from '@erpx/inventory-server/public/finance-record-sources';
import {purchasedCapabilityPredicate} from '@erpx/commercial-server/public/purchased-capability-sql';
import { scheduledFeaturePredicate } from "@erpx/pms-server/public/feature-availability";
export const manufacturingInitialStockJournalCapabilities=['manufacturing.materials','manufacturing.costs','inventory.stock-core','finance.production-costing'] as const;
export const manufacturingInitialStockJournalMarkedPredicate=(alias:string)=>`${rootOwnerExistsSql(bookRootUnionSql(alias), inventoryFinanceRecordSources.manufacturingInitialPhysicalCountMarkedJournal, 'marker')}`;
export const manufacturingInitialStockJournalPredicate=(alias:string)=>`${rootOwnerExistsSql(bookRootUnionSql(alias), inventoryFinanceRecordSources.manufacturingInitialPhysicalCountJournal, 'source')}`;
export const manufacturingInitialStockJournalReadPredicate=(alias:string,clock:string)=>`(${manufacturingInitialStockJournalPredicate(alias)} AND ${manufacturingInitialStockJournalCapabilities.map(cap=>purchasedCapabilityPredicate(`${alias}.company_id`,cap,'history',clock)).join(' AND ')} AND ${['manufacturing','inventory','finance'].map(module=>scheduledFeaturePredicate(`${alias}.company_id`,module)).join(' AND ')})`;
export async function manufacturingInitialStockJournalMarked(database:SqlDatabase,companyId:string,id:string){return !!await database.prepare(`${JOURNAL_SELECT_PREFIX}s d WHERE d.id=? AND d.company_id=? AND ${manufacturingInitialStockJournalMarkedPredicate('d')}`).bind(id,companyId).first();}
