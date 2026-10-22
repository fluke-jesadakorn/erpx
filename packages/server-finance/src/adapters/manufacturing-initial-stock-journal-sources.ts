import 'server-only';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {inventoryFinanceRecordSources} from '@erpx/server-inventory/public/finance-record-sources';
import {purchasedCapabilityPredicate} from '@erpx/server-commercial/public/purchased-capability-sql';
import { scheduledFeaturePredicate } from "@erpx/server-pms/public/feature-availability";
export const manufacturingInitialStockJournalCapabilities=['manufacturing.materials','manufacturing.costs','inventory.stock-core','finance.production-costing'] as const;
const roots=(alias:string)=>`SELECT ${alias}.id,${alias}.company_id UNION SELECT book.primary_journal_id,${alias}.company_id FROM finance_book_postings book WHERE book.secondary_journal_id=${alias}.id AND book.company_id=${alias}.company_id`;
export const manufacturingInitialStockJournalMarkedPredicate=(alias:string)=>`EXISTS(SELECT 1 FROM (${roots(alias)}) root JOIN (${inventoryFinanceRecordSources.manufacturingInitialPhysicalCountMarkedJournal}) marker ON marker.id=root.id AND marker.company_id=root.company_id)`;
export const manufacturingInitialStockJournalPredicate=(alias:string)=>`EXISTS(SELECT 1 FROM (${roots(alias)}) root JOIN (${inventoryFinanceRecordSources.manufacturingInitialPhysicalCountJournal}) source ON source.id=root.id AND source.company_id=root.company_id)`;
export const manufacturingInitialStockJournalReadPredicate=(alias:string,clock:string)=>`(${manufacturingInitialStockJournalPredicate(alias)} AND ${manufacturingInitialStockJournalCapabilities.map(cap=>purchasedCapabilityPredicate(`${alias}.company_id`,cap,'history',clock)).join(' AND ')} AND ${['manufacturing','inventory','finance'].map(module=>scheduledFeaturePredicate(`${alias}.company_id`,module)).join(' AND ')})`;
export async function manufacturingInitialStockJournalMarked(database:SqlDatabase,companyId:string,id:string){return !!await database.prepare(`SELECT 1 FROM finance_journals d WHERE d.id=? AND d.company_id=? AND ${manufacturingInitialStockJournalMarkedPredicate('d')}`).bind(id,companyId).first();}
