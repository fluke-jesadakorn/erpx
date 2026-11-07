import {readFileSync} from 'node:fs';
import {join} from 'node:path';
import {describe,expect,it,vi} from 'vitest';
vi.mock('server-only',()=>({}));
import {initialPhysicalStockFinanceJournalFilter} from '../../../packages/server-finance/src/adapters/document-purchases';

describe('native initial-stock journal filter',()=>{
 it('stays byte-identical to the Finance owner predicate',()=>{
  const generated=initialPhysicalStockFinanceJournalFilter('j','now_ms');
  const embedded=readFileSync(join(process.cwd(),'packages/server-finance/src/adapters/initial_stock_journal_filter.sql'),'utf8').trimEnd();
  expect(embedded).toBe(generated);
 });
});
