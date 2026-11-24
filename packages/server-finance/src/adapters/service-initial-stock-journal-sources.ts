import 'server-only';
import { JOURNAL_SELECT_PREFIX } from './finance-read-tail-r30-sql';
import {rootOwnerExistsSql} from './finance-read-tail-r19-sql';
import {bookRootUnionSql} from './finance-read-tail-r23-sql';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {inventoryFinanceRecordSources} from '@erpx/server-inventory/public/finance-record-sources';
import {purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
export const serviceInitialStockJournalCapabilities=['service.orders','service.parts-assets','finance.service-invoice'] as const;
export const serviceInitialStockJournalMarkedPredicate=(alias:string)=>`${rootOwnerExistsSql(bookRootUnionSql(alias), inventoryFinanceRecordSources.serviceInitialPhysicalCountMarkedJournal, 'marker')}`;
export const serviceInitialStockJournalPredicate=(alias:string)=>`${rootOwnerExistsSql(bookRootUnionSql(alias), inventoryFinanceRecordSources.serviceInitialPhysicalCountJournal, 'source')}`;
export const serviceInitialStockJournalReadPredicate=(alias:string,clock:string)=>`(${serviceInitialStockJournalPredicate(alias)} AND ${serviceInitialStockJournalCapabilities.map(cap=>purchasedCapabilityPredicate(`${alias}.company_id`,cap,'history',clock)).join(' AND ')} AND ${['field_service','inventory','finance'].map(module=>scheduledFeaturePredicate(`${alias}.company_id`,module)).join(' AND ')})`;
export async function serviceInitialStockJournalMarked(database:SqlDatabase,companyId:string,id:string){return !!await database.prepare(`${JOURNAL_SELECT_PREFIX}s d WHERE d.id=? AND d.company_id=? AND ${serviceInitialStockJournalMarkedPredicate('d')}`).bind(id,companyId).first();}
