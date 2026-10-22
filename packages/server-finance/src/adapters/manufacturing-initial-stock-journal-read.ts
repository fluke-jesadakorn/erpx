import {withServiceInitialStockJournalRead} from '@erpx/server-finance/adapters/service-initial-stock-journal-read';
import {serviceInitialStockJournalMarked} from '@erpx/server-finance/adapters/service-initial-stock-journal-sources';
import {withPosInitialStockJournalRead} from '@erpx/server-finance/adapters/pos-initial-stock-journal-read';
import {posInitialStockJournalMarked} from '@erpx/server-finance/adapters/pos-initial-stock-journal-sources';
import 'server-only';
import type {WorkContext} from '@erpx/server-identity_access/public/authorization';
import {lockQueuedCommandAuthority} from '@erpx/server-identity_access/public/queued-command-authority';
import type {SqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {isTransactionalSqlDatabase} from '@erpx/server-runtime/public/runtime/types';
import {holdingResources} from '../../../../packages/server-runtime/src/public/runtime';
import {ApiError} from '@erpx/server-runtime/public/http';
import {companyPurchaseReadLocks} from '@erpx/server-commercial/public/purchase-command-boundary';
import {lockScheduledFeatureSources} from '@erpx/server-pms/public/scheduled-feature-source-locks';
import {manufacturingInitialStockJournalReadSourceLocks,financeAttachmentReadSourceImage} from '@erpx/server-finance/adapters/attachment-read-sources';
import {manufacturingInitialStockJournalMarked} from '@erpx/server-finance/adapters/manufacturing-initial-stock-journal-sources';
import {requireFinanceDocumentHistory} from '@erpx/server-finance/adapters/document-purchases';
import {requireFinanceCurrentAuthority} from '@erpx/server-finance/adapters/purchase-boundary';
import {flushFinanceDeferredEffects} from '@erpx/server-finance/adapters/finance-transactions';
const reads=new WeakMap<SqlDatabase,{actor:WorkContext;id:string}>();
export async function withManufacturingInitialStockJournalRead<Result>(actor:WorkContext,id:string,read:(database:SqlDatabase)=>Promise<Result>,provided?:SqlDatabase):Promise<Result>{
 const owner=(await holdingResources(actor.holdingId)).write,database=provided??owner,borrowed=reads.get(database);
 if(borrowed){if(borrowed.actor!==actor||borrowed.id!==id)throw new ApiError(503,'MANUFACTURING_INITIAL_STOCK_READ_MISMATCH','A different journal cannot borrow this retained source read.');return read(database);}
 if(await serviceInitialStockJournalMarked(database,actor.companyId,id))return withServiceInitialStockJournalRead(actor,id,read,provided);
 if(await posInitialStockJournalMarked(database,actor.companyId,id))return withPosInitialStockJournalRead(actor,id,read,provided);
 if(!await manufacturingInitialStockJournalMarked(database,actor.companyId,id))return read(database);
 const execute=async(transaction:SqlDatabase)=>{
  await transaction.batch(manufacturingInitialStockJournalReadSourceLocks(transaction,actor.companyId));
  await lockQueuedCommandAuthority(transaction);await lockScheduledFeatureSources(transaction);await transaction.batch(companyPurchaseReadLocks(transaction,actor.companyId).slice(1));
  if(!await manufacturingInitialStockJournalMarked(transaction,actor.companyId,id))throw new ApiError(409,'MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED','The selected Manufacturing Count journal source changed before its locked read.');
  await requireFinanceCurrentAuthority(transaction,actor,`manufacturing-initial-stock-journal:${crypto.randomUUID()}`);
  await requireFinanceDocumentHistory(transaction,actor,'journal',id);
  const before=await financeAttachmentReadSourceImage(transaction,actor.companyId,'finance_journal',id);
  if(!before)throw new ApiError(404,'JOURNAL_NOT_FOUND','The original Manufacturing Count journal is unavailable.');
  reads.set(transaction,{actor,id});let result:Result;
  try{result=await read(transaction);}finally{reads.delete(transaction);}
  await flushFinanceDeferredEffects(transaction);
  await requireFinanceCurrentAuthority(transaction,actor,`manufacturing-initial-stock-journal-final:${crypto.randomUUID()}`);
  await requireFinanceDocumentHistory(transaction,actor,'journal',id);
  if(!await manufacturingInitialStockJournalMarked(transaction,actor.companyId,id)||before!==await financeAttachmentReadSourceImage(transaction,actor.companyId,'finance_journal',id))throw new ApiError(409,'MANUFACTURING_INITIAL_PHYSICAL_STOCK_SOURCE_CHANGED','The original Count or journal changed before the retained read completed.');
  return result;
 };
 if(database===owner){if(!isTransactionalSqlDatabase(owner))throw new ApiError(503,'TRANSACTION_REQUIRED','This retained journal requires its original Holding transaction.');return owner.transaction(execute);}
 const transactionOwner=owner as SqlDatabase&{assertActiveTransaction?:(tx:SqlDatabase)=>void};
 if(!transactionOwner.assertActiveTransaction)throw new ApiError(503,'TRANSACTION_OWNERSHIP_REQUIRED','This retained journal requires its owned Holding transaction.');
 transactionOwner.assertActiveTransaction(database);return execute(database);
}
