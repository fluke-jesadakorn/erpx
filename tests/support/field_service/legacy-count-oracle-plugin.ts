import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {transformWithOxc,type Plugin} from 'vite';

// Fixed test-only graph from the pre-fence source. Never aliases normal imports.
const modules={
 sql:['service-initial-physical-stock-count-source-sql.ts.snapshot','5c0052fad851b80fa136ed72d9d844f2b6b426012bbf26a6aeeb3dd84ae6095d'],
 domain:['service-initial-physical-stock-count-domain.ts.snapshot','eca377fe539e8285769ea369549cae01545c49a4fe488042a9a5473c4bea08d1'],
 service:['service-initial-physical-stock-count.ts.snapshot','7049ef1e277dcb737313f032f0ee16657d2532cfd4b8b184619d19d7d25dd2c7'],
 inventory:['inventory-counts.ts.snapshot','ab76f9ba70df7bd152646f803b2489c840bea3f00914d20e79e2e021951ccc06'],
 purchases:['count-purchases.ts.snapshot','36616c99b0453d1a3bd89916edd1533a05eb3ee8b84246aa97fa16c0d605ead8'],
 source:['service-initial-physical-stock-count-source.ts.snapshot','8c8bf0709b1e827e404678fa408f86f4dc83374371bd50d2dc7039ebcc098a98'],
 flow:['flow-routing.ts.snapshot','f9c8cfdded42dbb8844e9809de09c95e58fbb7e93bae0a7f6ed9141355ab0d6e'],
} as const;
const entry='virtual:service-count-historical',prefix='\0service-count-historical:';
const aliases:Record<string,keyof typeof modules>={
 '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source-sql':'sql',
 '@erpx/server-inventory/domain/service-initial-physical-stock-count':'domain',
 '@erpx/server-inventory/adapters/inventory-counts':'inventory',
 '@erpx/server-inventory/adapters/count-purchases':'purchases',
 '@erpx/server-inventory/public/count-purchases':'purchases',
 '@erpx/server-inventory/adapters/service-initial-physical-stock-count-source':'source',
 '@erpx/server-workflow/public/flow-routing':'flow',
 '@erpx/server-workflow/adapters/flow-routing':'flow',
};
export function historicalServiceCountOracle():Plugin{return {
 name:'test-only-service-count-historical-oracle',
 resolveId(id,importer){
  if(id===entry&&importer?.endsWith('/tests/support/field_service/legacy-count-oracle.ts'))return prefix+'service.ts';
  if(importer?.startsWith(prefix)&&id.startsWith(entry+'/')){const name=id.slice(entry.length+1);if(Object.hasOwn(modules,name))return prefix+name+'.ts';}
  if(importer?.startsWith(prefix)&&aliases[id])return prefix+aliases[id]+'.ts';
 },
 async load(id){
  if(!id.startsWith(prefix))return;
  const name=id.slice(prefix.length).replace(/\.ts$/,'') as keyof typeof modules, spec=modules[name];if(!spec)throw Error('LEGACY_COUNT_ORACLE_MODULE_REQUIRED');
  const bytes=readFileSync(resolve('fixtures/inventory/legacy-count-oracle',spec[0]));
  if(createHash('sha256').update(bytes).digest('hex')!==spec[1])throw Error('LEGACY_COUNT_ORACLE_SOURCE_CHANGED');
  let source=bytes.toString('utf8').replaceAll('../../../../packages/server-runtime/src/public/runtime','@erpx/server-runtime/public/runtime');
  for(const [alias,target] of Object.entries(aliases))for(const quote of ["'",'"'])source=source.replaceAll(`${quote}${alias}${quote}`,`${quote}${entry}/${target}${quote}`);
  return (await transformWithOxc(source,spec[0].replace(/\.snapshot$/,''),{lang:'ts'})).code;
 },
};}
