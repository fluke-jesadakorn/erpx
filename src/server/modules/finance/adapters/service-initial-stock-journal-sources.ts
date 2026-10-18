import 'server-only';
import type {SqlDatabase} from '../../runtime/public/runtime/types';
import {inventoryFinanceRecordSources} from '../../inventory/public/finance-record-sources';
import {purchasedCapabilityPredicate} from '../../commercial/public/purchased-capability-sql';
import {scheduledFeaturePredicate} from '../../pms/public/hospitality-rollout';
export const serviceInitialStockJournalCapabilities=['service.orders','service.parts-assets','finance.service-invoice'] as const;
const roots=(alias:string)=>`SELECT ${alias}.id,${alias}.company_id UNION SELECT book.primary_journal_id,${alias}.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=${alias}.id AND book.company_id=${alias}.company_id`;
export const serviceInitialStockJournalMarkedPredicate=(alias:string)=>`EXISTS(SELECT 1 FROM (${roots(alias)}) root JOIN (${inventoryFinanceRecordSources.serviceInitialPhysicalCountMarkedJournal}) marker ON marker.id=root.id AND marker.company_id=root.company_id)`;
export const serviceInitialStockJournalPredicate=(alias:string)=>`EXISTS(SELECT 1 FROM (${roots(alias)}) root JOIN (${inventoryFinanceRecordSources.serviceInitialPhysicalCountJournal}) source ON source.id=root.id AND source.company_id=root.company_id)`;
export const serviceInitialStockJournalReadPredicate=(alias:string,clock:string)=>`(${serviceInitialStockJournalPredicate(alias)} AND ${serviceInitialStockJournalCapabilities.map(cap=>purchasedCapabilityPredicate(`${alias}.company_id`,cap,'history',clock)).join(' AND ')} AND ${['field_service','inventory','finance'].map(module=>scheduledFeaturePredicate(`${alias}.company_id`,module)).join(' AND ')})`;
export async function serviceInitialStockJournalMarked(database:SqlDatabase,companyId:string,id:string){return !!await database.prepare(`SELECT 1 FROM finance_journals d WHERE d.id=? AND d.company_id=? AND ${serviceInitialStockJournalMarkedPredicate('d')}`).bind(id,companyId).first();}
