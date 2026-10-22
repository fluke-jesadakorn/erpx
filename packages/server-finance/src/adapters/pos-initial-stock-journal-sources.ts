import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {inventoryFinanceRecordSources} from '@erpx/server-inventory/public/finance-record-sources';
import {purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
export const posInitialStockJournalCapabilities=['pos.sales','inventory.stock-core','finance.pos-posting'] as const;
const roots=(alias:string)=>`SELECT ${alias}.id,${alias}.company_id UNION SELECT book.primary_journal_id,${alias}.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=${alias}.id AND book.company_id=${alias}.company_id`;
export const posInitialStockJournalMarkedPredicate=(alias:string)=>`EXISTS(SELECT 1 FROM (${roots(alias)}) root JOIN (${inventoryFinanceRecordSources.posInitialPhysicalCountMarkedJournal}) marker ON marker.id=root.id AND marker.company_id=root.company_id)`;
export const posInitialStockJournalPredicate=(alias:string)=>`EXISTS(SELECT 1 FROM (${roots(alias)}) root JOIN (${inventoryFinanceRecordSources.posInitialPhysicalCountJournal}) source ON source.id=root.id AND source.company_id=root.company_id)`;
export const posInitialStockJournalReadPredicate=(alias:string,clock:string)=>`(${posInitialStockJournalPredicate(alias)} AND ${posInitialStockJournalCapabilities.map(cap=>purchasedCapabilityPredicate(`${alias}.company_id`,cap,'history',clock)).join(' AND ')} AND ${['pos','inventory','finance'].map(module=>scheduledFeaturePredicate(`${alias}.company_id`,module)).join(' AND ')})`;
export async function posInitialStockJournalMarked(database:SqlDatabase,companyId:string,id:string){return !!await database.prepare(`SELECT 1 FROM finance_journals d WHERE d.id=? AND d.company_id=? AND ${posInitialStockJournalMarkedPredicate('d')}`).bind(id,companyId).first();}
