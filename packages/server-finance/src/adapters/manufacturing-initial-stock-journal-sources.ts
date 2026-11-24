import 'server-only';
import {rootOwnerExistsSql} from './finance-read-tail-r19-sql';
import {bookRootUnionSql} from './finance-read-tail-r23-sql';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {inventoryFinanceRecordSources} from '@erpx/server-inventory/public/finance-record-sources';
import {purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
export const manufacturingInitialStockJournalCapabilities=['manufacturing.materials','manufacturing.costs','inventory.stock-core','finance.production-costing'] as const;
export const manufacturingInitialStockJournalMarkedPredicate=(alias:string)=>`${rootOwnerExistsSql(bookRootUnionSql(alias), inventoryFinanceRecordSources.manufacturingInitialPhysicalCountMarkedJournal, 'marker')}`;
export const manufacturingInitialStockJournalPredicate=(alias:string)=>`${rootOwnerExistsSql(bookRootUnionSql(alias), inventoryFinanceRecordSources.manufacturingInitialPhysicalCountJournal, 'source')}`;
export const manufacturingInitialStockJournalReadPredicate=(alias:string,clock:string)=>`(${manufacturingInitialStockJournalPredicate(alias)} AND ${manufacturingInitialStockJournalCapabilities.map(cap=>purchasedCapabilityPredicate(`${alias}.company_id`,cap,'history',clock)).join(' AND ')} AND ${['manufacturing','inventory','finance'].map(module=>scheduledFeaturePredicate(`${alias}.company_id`,module)).join(' AND ')})`;
export async function manufacturingInitialStockJournalMarked(database:SqlDatabase,companyId:string,id:string){return !!await database.prepare(`SELECT 1 FROM finance_journals d WHERE d.id=? AND d.company_id=? AND ${manufacturingInitialStockJournalMarkedPredicate('d')}`).bind(id,companyId).first();}
