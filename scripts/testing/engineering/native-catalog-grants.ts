/** Test-only bootstrap for an explicitly owned, network-isolated PostgreSQL container. */
import {execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {validateNativeEngineeringRoster,type NativeEngineeringRoster} from '@erpx/server-runtime/public/native-engineering-roster';
const exec=promisify(execFile);
export async function grantNativeEngineeringCatalogLocks(input:{dockerHost:string;containerId:string;run:string;nonce:string;sourceFingerprint:string;fileManifestSha256:string;roster:NativeEngineeringRoster}){
 if(!/^unix:\/\//.test(input.dockerHost)||!/^[a-f0-9]{64}$/.test(input.containerId)||!/^[a-f0-9]{32}$/.test(input.nonce)||!/^[a-f0-9]{12}$/.test(input.run)||!validateNativeEngineeringRoster(input.roster,input.run,input.sourceFingerprint,input.fileManifestSha256))throw Error('NATIVE_CATALOG_GRANT_INPUT_INVALID');
 const env={PATH:process.env.PATH??'',DOCKER_HOST:input.dockerHost,NODE_ENV:'test' as const};
 const docker=async(args:string[])=>exec('docker',args,{env,maxBuffer:1024*1024});
 const inspected=JSON.parse((await docker(['inspect',input.containerId])).stdout) as Array<{Id:string;State:{Running:boolean};Image:string;Mounts:unknown[];Config:{Labels:Record<string,string>;Env:string[]};HostConfig:{NetworkMode:string;PortBindings:Record<string,unknown>|null;Tmpfs:Record<string,string>|null;Binds:string[]|null;Privileged:boolean;PidMode:string}}>;
 const container=inspected[0],labels=container?.Config.Labels;
 if(inspected.length!==1||container?.Id!==input.containerId||container.Image!=='sha256:979bccfde35445db618c65107d27d08c6c21339d8dc3ad4c830c6633947be0e4'||!container.State.Running||container.Mounts.length!==0||(container.HostConfig.Binds?.length??0)!==0||container.HostConfig.Privileged||container.HostConfig.PidMode==='host'||container.Config.Env.some(value=>/^(?:PGHOST|PGPORT|PGUSER|PGDATABASE|PGOPTIONS)=/.test(value))||container.HostConfig.NetworkMode!=='none'||Object.keys(container.HostConfig.PortBindings??{}).length!==0||!container.HostConfig.Tmpfs?.['/var/lib/postgresql']||labels?.['com.erpx.native-engineering-run']!==input.run||labels?.['com.erpx.native-engineering-nonce']!==input.nonce)throw Error('NATIVE_CATALOG_GRANT_CONTAINER_INVALID');
 const command=async(database:string,sql:string)=>docker(['exec',input.containerId,'psql','-XAt','-h','/var/run/postgresql','-p','5432','-v','ON_ERROR_STOP=1','-U','postgres','-d',database,'-c',sql]);
 // Check all physical identities before granting even the first privilege.
 for(const e of input.roster.entries){
  const name=`erpx_dev_rust_${e.token}`;
  const check=`SELECT (d.oid=${e.oid}::oid AND pg_get_userbyid(d.datdba)='erpx_test_admin' AND shobj_description(d.oid,'pg_database')='erpx-rust-test:${e.token}' AND current_database()=d.datname AND floor(extract(epoch FROM pg_postmaster_start_time())*1000)::bigint=${input.roster.postmasterStartedMs} AND NOT r.rolsuper AND NOT r.rolcreaterole AND NOT r.rolreplication AND NOT r.rolbypassrls AND r.rolcanlogin AND r.rolcreatedb AND NOT EXISTS(SELECT 1 FROM aclexplode(COALESCE(d.datacl,acldefault('d',d.datdba))) a WHERE a.grantee=0 AND a.privilege_type='CONNECT')) FROM pg_database d JOIN pg_roles r ON r.rolname='erpx_test_admin' WHERE d.datname=current_database()`;
  if((await command(name,check)).stdout.trim()!=='t')throw Error('NATIVE_CATALOG_GRANT_DATABASE_INVALID');
 }
 for(const e of input.roster.entries){
  const name=`erpx_dev_rust_${e.token}`;
  await command(name,'GRANT MAINTAIN ON pg_catalog.pg_database,pg_catalog.pg_shdescription TO erpx_test_admin');
  const check="SELECT has_table_privilege('erpx_test_admin','pg_catalog.pg_database','MAINTAIN') AND has_table_privilege('erpx_test_admin','pg_catalog.pg_shdescription','MAINTAIN') AND NOT has_table_privilege('erpx_test_admin','pg_catalog.pg_database','UPDATE') AND NOT has_table_privilege('erpx_test_admin','pg_catalog.pg_shdescription','UPDATE') AND NOT has_table_privilege('erpx_test_admin','pg_catalog.pg_database','DELETE') AND NOT has_table_privilege('erpx_test_admin','pg_catalog.pg_shdescription','DELETE') AND NOT has_table_privilege('erpx_test_admin','pg_catalog.pg_database','INSERT') AND NOT has_table_privilege('erpx_test_admin','pg_catalog.pg_shdescription','INSERT') AND NOT has_table_privilege('erpx_test_admin','pg_catalog.pg_database','TRUNCATE') AND NOT has_table_privilege('erpx_test_admin','pg_catalog.pg_shdescription','TRUNCATE')";
  if((await command(name,check)).stdout.trim()!=='t')throw Error('NATIVE_CATALOG_GRANT_VERIFICATION_FAILED');
 }
}
