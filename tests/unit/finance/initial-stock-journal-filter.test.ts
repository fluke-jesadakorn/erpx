import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {join} from 'node:path';
import {describe,expect,it,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {initialPhysicalStockFinanceJournalFilter} from '../../../modules/finance/server/src/adapters/document-purchases';
import {rootOwnerExistsSql} from '../../../modules/finance/server/src/adapters/finance-read-tail-r19-sql';
import {inventoryFinanceRecordSources} from '@erpx/inventory-server/public/finance-record-sources';
import {serviceInitialStockJournalPredicate} from '@erpx/finance-server/adapters/service-initial-stock-journal-sources';
import {posInitialStockJournalPredicate} from '@erpx/finance-server/adapters/pos-initial-stock-journal-sources';
import {manufacturingInitialStockJournalPredicate} from '@erpx/finance-server/adapters/manufacturing-initial-stock-journal-sources';
import {assignmentReadSql} from '@erpx/identity-access-server/public/record-source-authority';

const policyPath=join(process.cwd(),'modules/finance/server/src/adapters/records_search_policy.rs');
const policy=()=>readFileSync(policyPath,'utf8');
function pinned(name:string){
 const match=policy().match(new RegExp(`const ${name}: &str = r##"([\\s\\S]*?)"##;`));
 if(!match)throw new Error(`missing ${name} pin`);
 return match[1];
}
const sha=(value:string)=>createHash('sha256').update(value).digest('hex');
const trimLineEnds=(value:string)=>value.replace(/[ \t]+$/gm,'');
const rootsJournal=(alias:string)=>`SELECT ${alias}.id,${alias}.company_id UNION SELECT p.id,p.company_id FROM finance_book_postings b JOIN finance_journals p ON p.id=b.primary_journal_id AND p.company_id=${alias}.company_id WHERE b.secondary_journal_id=${alias}.id`;
const initialPredicate=()=>{
 const sales=`${rootOwnerExistsSql(rootsJournal('d'),inventoryFinanceRecordSources.salesInitialPhysicalCountJournal,'source')}`;
 return `EXISTS(SELECT 1 FROM finance_journals d WHERE d.id=workspace_records.id AND d.company_id=workspace_records.company_id AND (${sales} OR ${manufacturingInitialStockJournalPredicate('d')} OR ${posInitialStockJournalPredicate('d')} OR ${serviceInitialStockJournalPredicate('d')}))`;
};

describe('native initial-stock journal filter',()=>{
 it('stays byte-identical to the Finance owner predicate',()=>{
  const generated=initialPhysicalStockFinanceJournalFilter('j','now_ms');
  const embedded=readFileSync(join(process.cwd(),'modules/finance/server/src/adapters/initial_stock_journal_filter.sql'),'utf8').trimEnd();
  expect(embedded).toBe(generated);
 });
 it('pins the exact native initial-source predicate',()=>{
  expect(pinned('INITIAL_STOCK_SQL')).toBe(trimLineEnds(initialPredicate()));
 });
 it('pins the assignment predicate before actor-CTE bind adaptation',()=>{
  expect(pinned('ASSIGNMENT_TS_SQL')).toBe(assignmentReadSql('workspace_records','finance.read'));
  expect(pinned('ASSIGNMENT_TS_SQL').match(/\?/g)).toHaveLength(3);
 });
 it('pins the dev scoped-alias transform to the authoritative filter',()=>{
  const expected=initialPhysicalStockFinanceJournalFilter('workspace_records','records_search_actor.now_ms');
  const match=policy().match(/assert_eq!\(\s*hex\(Sha256::digest\(sql\.as_bytes\(\)\)\),\s*"([0-9a-f]{64})"\s*\)/);
  expect(match).not.toBeNull();
  expect(match![1]).toBe(sha(expected));
 });
 it('pins the production scheduled-rollout filter instead of dev TRUE',()=>{
  vi.stubEnv('NODE_ENV','production');
  try{
   const expected=initialPhysicalStockFinanceJournalFilter('workspace_records','records_search_actor.now_ms');
   expect(expected).toContain('company_feature_rollouts');
   expect(pinned('ROW_FILTER_PRODUCTION_SQL')).toBe(trimLineEnds(expected));
  }finally{
   vi.unstubAllEnvs();
  }
 });
 it('keeps the owner port bind-free and mode-exact',()=>{
  const source=policy();
  expect(source).toContain('records_search_actor.assignment_id');
  expect(source).toContain('records_search_actor.user_id');
  expect(source).toContain('records_search_actor.role');
  expect(source).toContain('(module_id<>\'finance\' OR ((CASE WHEN ');
  expect(source).toContain(' AND TRUE END)))');
  expect(source).not.toContain('finance_search_clock.now_ms');
 });
});
