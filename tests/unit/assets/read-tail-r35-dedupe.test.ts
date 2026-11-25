import {describe,it,expect,vi} from 'vitest';
import {readFileSync,readdirSync} from 'node:fs';
import {resolve} from 'node:path';

vi.mock('server-only',()=>({}));

const srcRoot=resolve(__dirname,'../../../packages/server-assets/src');
const src=(p:string)=>readFileSync(resolve(srcRoot,p),'utf8');
const ownerSelectCount=()=>{
 let selects=0;
 const walk=(dir:string)=>{
  for(const entry of readdirSync(dir,{withFileTypes:true})){
   const path=resolve(dir,entry.name);
   if(entry.isDirectory())walk(path);
   else if(entry.name.endsWith('.ts'))selects+=(readFileSync(path,'utf8').match(/SELECT/g)??[]).length;
  }
 };
 walk(srcRoot);
 return selects;
};

describe('Assets read-tail r35 scope-tuple universal head family',()=>{
 it('renders the work-scope tuple byte-identically from the one universal head',async()=>{
  const {assetScopeTupleSql,assetSelectProjectionSql}=await import('../../../packages/server-assets/src/adapters/asset-scope-tuple');
  // Pre-r35 tuple literals, rebuilt verbatim; no projection, alias or bind changes.
  const tuple=(alias:string)=>`(SELECT ?::text company_id,?::text branch_id,?::text department_id) ${alias}`;
  for(const alias of ['asset_file_scope','workforce_actor_scope','asset_record_source']){
   expect(assetScopeTupleSql(alias)).toBe(tuple(alias));
   expect(assetScopeTupleSql(alias))
    .toBe(`(${assetSelectProjectionSql('?::text company_id,?::text branch_id,?::text department_id')}) ${alias}`);
  }
  // The other delegates keep rendering byte-identically from the same head.
  expect(assetSelectProjectionSql('1','::text image')).toBe('SELECT 1::text image');
  expect(assetSelectProjectionSql('id,version',' FROM operational_assets WHERE id=?')).toBe('SELECT id,version FROM operational_assets WHERE id=?');
 });

 it('keeps every real scope-tuple call site byte-identical with unchanged binds',async()=>{
  const {assetScopeTupleSql,assetProjectionFromSql,assetCurrentScopeProbeSql}=await import('../../../packages/server-assets/src/adapters/asset-scope-tuple');
  const {assignmentReadSql}=await import('@erpx/server-identity_access/public/record-source-authority');
  const tuple=(alias:string)=>`(SELECT ?::text company_id,?::text branch_id,?::text department_id) ${alias}`;
  // Attachment authority assertion: the FROM fragment still carries the same tuple.
  const authorityFrom=` ${assetScopeTupleSql('asset_file_scope')}`;
  expect(authorityFrom).toBe(` ${tuple('asset_file_scope')}`);
  expect(assetCurrentScopeProbeSql(assignmentReadSql('asset_file_scope','records.write'),authorityFrom))
   .toBe(`SELECT ${assignmentReadSql('asset_file_scope','records.write')} FROM ${tuple('asset_file_scope')}`);
  // Current-scope check and the assignment-allowed probe keep the same tuple alias.
  expect(assetCurrentScopeProbeSql('1',assetScopeTupleSql('workforce_actor_scope'),' WHERE scope.active'))
   .toBe(`SELECT 1 FROM${tuple('workforce_actor_scope')} WHERE scope.active`);
  expect(assetProjectionFromSql(`${assignmentReadSql('scope','records.read')} allowed`,` ${assetScopeTupleSql('scope')}`))
   .toBe(`SELECT ${assignmentReadSql('scope','records.read')} allowed FROM ${tuple('scope')}`);
  // Three tuple binds remain in the same company/branch/department order.
  expect((assetScopeTupleSql('scope').match(/\?::text/g)??[])).toHaveLength(3);
 });

 it('owns the universal head in the Rust JSON contract with no local statement token',()=>{
  const scope=src('adapters/asset-scope-tuple.ts');
  expect((scope.match(/SELECT/g)??[])).toHaveLength(0);
  expect(scope).toContain('import contract from "./asset-projection-head-sources.json" with { type: "json" }');
  expect(scope).toContain('heads.selectHead.replace("{projection}",projection).replace("{tail}",tail)');
  expect(scope).toContain('heads.fromHead.replace("{from}",from).replace("{tail}",tail)');
  expect(scope).toContain('export const assetSelectProjectionSql = (projection:string,tail="") =>');
  expect(scope).toContain('export const assetScopeTupleSql = (alias: string) =>');
  expect(scope).toContain('assetSelectProjectionSql("?::text company_id,?::text branch_id,?::text department_id")');
  expect(scope.split('(SELECT ?::text company_id,?::text branch_id,?::text department_id)').length-1).toBe(0);
 });

 it('lowers the exact owner TS SELECT tail from BEFORE=3 to KPI 0 and leaves excluded families untouched',()=>{
  expect(ownerSelectCount()).toBe(0);
  // The universal head and the cross-owner clock scalar are both owner-planned.
  expect((src('adapters/asset-scope-tuple.ts').match(/SELECT/g)??[])).toHaveLength(0);
  expect((src('adapters/record-purchases.ts').match(/SELECT/g)??[])).toHaveLength(0);
  expect(src('adapters/record-purchases.ts')).toContain('await nativeAssetsClockScalar()');
  // Locks, native plans and earlier families are not re-opened.
  expect(src('adapters/financial-workflow-sources.ts')).toContain('LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links,maintenance_work_orders IN SHARE MODE');
  expect(src('adapters/purchases.ts')).toContain('LOCK TABLE operational_assets,operational_asset_finance_links IN SHARE ROW EXCLUSIVE MODE');
  expect(src('adapters/package-obligations.ts')).toContain('LOCK TABLE operational_assets,operational_asset_events,operational_asset_finance_links IN SHARE MODE');
  expect(src('adapters/native-assets-read.ts')).toContain('ASSETS_RD_OBLIGATIONS_BASE');
  expect(src('adapters/native-assets-read.ts')).toContain('ASSETS_RD_FS_ASSET_ATTACHMENT_IMAGE');
 });
});
