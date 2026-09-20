import { expect,it } from 'vitest';
import { env } from './runtime-env';
import { readListPage } from '../src/server/list-page';
const sql = `SELECT lpad(n::text,4,'0') AS id, (n/3)::text AS page_key FROM generate_series(1,95) n`;
it('pages duplicate sort keys forward and backward without losing rows',async()=>{
  const db=env.CONTROL_DB;
  const first=await readListPage(db,{sql,args:[],scope:['test']});
  const second=await readListPage(db,{sql,args:[],scope:['test'],cursor:first.nextCursor!});
  const back=await readListPage(db,{sql,args:[],scope:['test'],cursor:second.previousCursor!});
  expect(first.items).toHaveLength(30);expect(back.items).toEqual(first.items);
  expect(first.previousCursor).toBeNull();
  const ids=first.items.map(r=>r.id);let page=second;
  for(;;){ids.push(...page.items.map(r=>r.id));if(!page.nextCursor)break;page=await readListPage(db,{sql,args:[],scope:['test'],cursor:page.nextCursor});}
  expect(ids).toHaveLength(95);expect(new Set(ids).size).toBe(95);
  await expect(readListPage(db,{sql,args:[],scope:['other'],cursor:first.nextCursor!})).rejects.toMatchObject({code:'INVALID_CURSOR'});
});
it('handles empty lists, descending order and exact page boundaries',async()=>{
  const db=env.CONTROL_DB;
  const empty=await readListPage(db,{sql:`${sql} WHERE false`,args:[],scope:[]});
  expect(empty.items).toEqual([]);expect(empty.nextCursor).toBeNull();
  const first=await readListPage(db,{sql,args:[],scope:[],descending:true,limit:30});
  const second=await readListPage(db,{sql,args:[],scope:[],descending:true,limit:30,cursor:first.nextCursor!});
  const back=await readListPage(db,{sql,args:[],scope:[],descending:true,limit:30,cursor:second.previousCursor!});
  expect(back.items).toEqual(first.items);
  const exact=await readListPage(db,{sql:`${sql} WHERE n<=30`,args:[],scope:[]});
  expect(exact.items).toHaveLength(30);expect(exact.nextCursor).toBeNull();
});
