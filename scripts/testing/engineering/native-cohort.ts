/** Disposable compiled-engineering cohort; never adopts or renames existing databases. */
import {createHash,randomBytes} from 'node:crypto';
import {lstat,realpath,readFile,writeFile,rm} from 'node:fs/promises';
import {join,resolve} from 'node:path';
import {Pool} from 'pg';
import {nativeEngineeringBindings,type NativeEngineeringRoster} from '@erpx/server-runtime/public/native-engineering-roster';

const ident=(s:string)=>{if(!/^erpx_dev_rust_[a-f0-9]{32}$/.test(s))throw Error('NATIVE_COHORT_NAME_INVALID');return `"${s}"`;};
export async function createNativeEngineeringCohort(input:{directory:string;run:string;sourceFingerprint:string;fileManifestSha256:string;adminUrl:string}){
 const {run,sourceFingerprint,fileManifestSha256}=input;
 if(!/^[a-f0-9]{12}$/.test(run)||!/^[a-f0-9]{64}$/.test(sourceFingerprint)||!/^[a-f0-9]{64}$/.test(fileManifestSha256))throw Error('NATIVE_COHORT_INPUT_INVALID');
 const directory=resolve(input.directory),info=await lstat(directory);
 if(await realpath(directory)!==directory||!directory.endsWith(`/.runtime/browser/${run}`)||!info.isDirectory()||info.isSymbolicLink()||(info.mode&0o077)!==0||info.uid!==process.getuid?.())throw Error('NATIVE_COHORT_DIRECTORY_INVALID');
 const release=join(directory,'release'),releaseInfo=join(release,'release.json'),releaseFiles=join(release,'release-files.json');
 if(await realpath(release)!==release||await realpath(releaseInfo)!==releaseInfo||await realpath(releaseFiles)!==releaseFiles)throw Error('NATIVE_COHORT_RELEASE_PATH_INVALID');
 const filesInfo=await lstat(releaseFiles),metadataInfo=await lstat(releaseInfo);
 if(!filesInfo.isFile()||filesInfo.isSymbolicLink()||filesInfo.size>16*1024*1024||!metadataInfo.isFile()||metadataInfo.isSymbolicLink()||metadataInfo.size>16384)throw Error('NATIVE_COHORT_RELEASE_INVALID');
 const releaseBytes=await readFile(releaseFiles),releaseMetadata=JSON.parse(await readFile(releaseInfo,'utf8'));
 if(releaseBytes.length>16*1024*1024||createHash('sha256').update(releaseBytes).digest('hex')!==fileManifestSha256||releaseMetadata.sourceFingerprint!==sourceFingerprint||releaseMetadata.softwareVersion!==`release-${sourceFingerprint}`||releaseMetadata.fileManifestSha256!==fileManifestSha256)throw Error('NATIVE_COHORT_RELEASE_INVALID');
 const admin=new URL(input.adminUrl);
 if(!['postgres:','postgresql:'].includes(admin.protocol)||admin.hostname!=='127.0.0.1'||admin.port!=='5432'||admin.username!=='erpx_test_admin'||!admin.password||admin.pathname!=='/postgres'||admin.search||admin.hash)throw Error('NATIVE_COHORT_ADMIN_INVALID');
 const pool=new Pool({connectionString:admin.href,max:1}),created:{name:string;token:string;oid:number;marked:boolean}[]=[];
 const paths=['native-engineering-leases.json','databases.json','databases-migrations.json'].map(p=>join(directory,p));let written:string[]=[];
 async function cleanup(){
  for(const item of [...created].reverse()){
   const rows=await pool.query("SELECT d.oid::int8 oid,pg_get_userbyid(d.datdba) owner,shobj_description(d.oid,'pg_database') lease,(SELECT count(*)::int FROM pg_stat_activity a WHERE a.datid=d.oid) active FROM pg_database d WHERE d.datname=$1",[item.name]);
   if(rows.rowCount===0)continue;const row=rows.rows[0];
   if(Number(row.oid)!==item.oid||row.owner!=='erpx_test_admin'||row.lease!==(item.marked?`erpx-rust-test:${item.token}`:null)||row.active!==0)throw Error('NATIVE_COHORT_CLEANUP_IDENTITY_CHANGED');
   await pool.query(`DROP DATABASE ${ident(item.name)}`);
  }
  for(const path of written)await rm(path,{force:true});written=[];
 }
 try{
  const role=(await pool.query("SELECT current_user::text role,session_user::text session_role,rolsuper,rolcreaterole,rolcreatedb,rolreplication,rolbypassrls,rolcanlogin,floor(extract(epoch FROM pg_postmaster_start_time())*1000)::bigint started FROM pg_roles WHERE rolname=current_user")).rows[0];
  if(!role||role.role!=='erpx_test_admin'||role.session_role!=='erpx_test_admin'||role.rolsuper||role.rolcreaterole||role.rolreplication||role.rolbypassrls||!role.rolcanlogin||!role.rolcreatedb)throw Error('NATIVE_COHORT_ADMIN_INVALID');
  const entries:NativeEngineeringRoster['entries']=[],databases:Record<string,string>={};
  for(const binding of nativeEngineeringBindings){
   const token=randomBytes(16).toString('hex'),name=`erpx_dev_rust_${token}`;await pool.query(`CREATE DATABASE ${ident(name)} TEMPLATE template0`);
   const row=(await pool.query('SELECT oid::int8 oid FROM pg_database WHERE datname=$1',[name])).rows[0];const item={name,token,oid:Number(row.oid),marked:false};created.push(item);
   await pool.query(`REVOKE CONNECT ON DATABASE ${ident(name)} FROM PUBLIC`);await pool.query(`COMMENT ON DATABASE ${ident(name)} IS 'erpx-rust-test:${token}'`);item.marked=true;
   entries.push({binding,token,oid:item.oid});const url=new URL(admin.href);url.pathname=name;databases[binding]=url.href;
  }
  const roster:NativeEngineeringRoster={version:1,run,sourceFingerprint,fileManifestSha256,postmasterStartedMs:Number(role.started),entries};
  const bytes=Buffer.from(JSON.stringify(roster));const slots=[1,2,3].map(i=>({id:`h00${i}`,write:`H00${i}_DB`,read:`H00${i}_READ_DB`,files:`H00${i}_FILES`}));
  for(const [path,data] of [[paths[0]!,bytes],[paths[1]!,JSON.stringify({version:1,databases,slots})],[paths[2]!,JSON.stringify({version:1,databases,slots})]] as const){await writeFile(path,data,{mode:0o600,flag:'wx'});written.push(path);}
  return {roster,rosterSha256:createHash('sha256').update(bytes).digest('hex'),databases,cleanup:async()=>{try{await cleanup();}finally{await pool.end();}}};
 }catch(error){try{await cleanup();}finally{await pool.end();}throw error;}
}
