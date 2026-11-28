import 'server-only';
import { JOURNAL_SELECT_PREFIX } from './finance-read-tail-r30-sql';
import {rootOwnerExistsSql} from './finance-read-tail-r19-sql';
import {bookRootUnionSql} from './finance-read-tail-r23-sql';
import type {SqlDatabase} from '@erpx/platform-runtime-server/public/runtime/types';
import {inventoryFinanceRecordSources} from '@erpx/inventory-server/public/finance-record-sources';
import {purchasedCapabilityPredicate} from '@erpx/commercial-server/public/purchased-capability-sql';
import { scheduledFeaturePredicate } from "@erpx/pms-server/public/feature-availability";
export const posInitialStockJournalCapabilities=['pos.sales','inventory.stock-core','finance.pos-posting'] as const;
export const posInitialStockJournalMarkedPredicate=(alias:string)=>`${rootOwnerExistsSql(bookRootUnionSql(alias), inventoryFinanceRecordSources.posInitialPhysicalCountMarkedJournal, 'marker')}`;
export const posInitialStockJournalPredicate=(alias:string)=>`${rootOwnerExistsSql(bookRootUnionSql(alias), inventoryFinanceRecordSources.posInitialPhysicalCountJournal, 'source')}`;
export const posInitialStockJournalReadPredicate=(alias:string,clock:string)=>`(${posInitialStockJournalPredicate(alias)} AND ${posInitialStockJournalCapabilities.map(cap=>purchasedCapabilityPredicate(`${alias}.company_id`,cap,'history',clock)).join(' AND ')} AND ${['pos','inventory','finance'].map(module=>scheduledFeaturePredicate(`${alias}.company_id`,module)).join(' AND ')})`;
export async function posInitialStockJournalMarked(database:SqlDatabase,companyId:string,id:string){return !!await database.prepare(`${JOURNAL_SELECT_PREFIX}s d WHERE d.id=? AND d.company_id=? AND ${posInitialStockJournalMarkedPredicate('d')}`).bind(id,companyId).first();}
