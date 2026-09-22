import { afterEach,expect,it } from "vitest";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { chmod,cp,mkdir,mkdtemp,readFile,readlink,rm,symlink,writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { join,resolve } from "node:path";
import { createHash,randomBytes } from "node:crypto";
import { Pool } from "pg";

const execute=promisify(execFile),temporary:string[]=[],databases:string[]=[];
const adminUrl=new URL(process.env.ERPX_TEST_ADMIN_URL!);
afterEach(async()=>{const admin=new Pool({connectionString:adminUrl.href,max:1});try{for(const name of databases.splice(0)){const owner=(await admin.query("SELECT pg_get_userbyid(datdba) owner FROM pg_database WHERE datname=$1",[name])).rows[0]?.owner;if(owner&&owner!=="erpx_test_admin")throw new Error("RESTORE_DATABASE_OWNER_CHANGED");await admin.query(`DROP DATABASE IF EXISTS "${name}"`);}}finally{await admin.end();}await Promise.all(temporary.splice(0).map(path=>rm(path,{recursive:true,force:true})));});

it("backs up and restores an isolated complete database, private files, and application secret",async()=>{
 if(adminUrl.hostname!=="127.0.0.1"||adminUrl.port!=="5432"||adminUrl.username!=="erpx_test_admin"||adminUrl.pathname!=="/postgres")throw new Error("LOCAL_TEST_ADMIN_REQUIRED");
 const suffix=randomBytes(5).toString("hex"),database=`erpx_test_${suffix}_backup`;databases.push(database);
 const admin=new Pool({connectionString:adminUrl.href,max:1});await admin.query(`CREATE DATABASE "${database}" TEMPLATE template0`);await admin.end();
 const databaseUrl=new URL(adminUrl.href);databaseUrl.pathname=`/${database}`;const db=new Pool({connectionString:databaseUrl.href,max:1});await db.query("CREATE TABLE recovery_probe(id text PRIMARY KEY,value text NOT NULL)");await db.query("INSERT INTO recovery_probe VALUES('authority','before-backup')");await db.end();
 const home=await mkdtemp(join(process.cwd(),".runtime/tests/full-recovery-"));temporary.push(home);const releaseId="2026-09-21T20-30-00Z",release=join(home,"releases",releaseId),pgBin=join(home,"runtime/postgres/bin");
 await Promise.all([mkdir(join(home,"data/config"),{recursive:true}),mkdir(join(home,"data/files/holding/evidence"),{recursive:true}),mkdir(join(home,"config"),{recursive:true}),mkdir(release,{recursive:true}),mkdir(pgBin,{recursive:true})]);
 for(const binary of ["pg_dump","pg_restore","psql"]){const found=(await execute("which",[binary])).stdout.trim();expect(found).toBeTruthy();await symlink(found,join(pgBin,binary));}
 await writeFile(join(home,"data/config/databases.json"),JSON.stringify({version:1,databases:{H001_DB:databaseUrl.href,H001_READ_DB:databaseUrl.href}}));
 await writeFile(join(home,"config/admin.json"),JSON.stringify({host:adminUrl.hostname,port:Number(adminUrl.port),user:decodeURIComponent(adminUrl.username),password:decodeURIComponent(adminUrl.password)}));
 await writeFile(join(home,"config/production.env"),"BETTER_AUTH_SECRET=before-backup\n",{mode:0o600});await chmod(join(home,"config/production.env"),0o600);
 await writeFile(join(home,"data/files/holding/evidence/document.txt"),"immutable file before backup\n");await writeFile(join(release,"release.json"),JSON.stringify({id:releaseId,schemaSha256:"schema-demo"}));await writeFile(join(release,"server.js"),"// recovery fixture\n");await symlink(release,join(home,"current"));
 const operator=resolve("ops/macos/erpxctl.py"),backup=(await execute("python3",[operator,"--home",home,"--offline-test","backup"],{timeout:120000})).stdout.trim(),snapshot=backup.match(/^Backup:\s+(.+)$/m)?.[1];expect(snapshot).toBeTruthy();
 const snapshotPath=String(snapshot),metadata=JSON.parse(await readFile(join(snapshotPath,"backup.json"),"utf8"));expect(metadata.databases).toEqual([database]);expect(JSON.parse(await readFile(join(snapshotPath,"checksums.json"),"utf8"))).toHaveProperty(`databases/${database}.dump`);
 const changed=new Pool({connectionString:databaseUrl.href,max:1});await changed.query("UPDATE recovery_probe SET value='after-backup'");await changed.end();await writeFile(join(home,"data/files/holding/evidence/document.txt"),"changed after backup\n");await writeFile(join(home,"config/production.env"),"BETTER_AUTH_SECRET=changed-after-backup\n");
 await execute("python3",[operator,"--home",home,"--offline-test","restore",snapshotPath],{timeout:120000});
 const restored=new Pool({connectionString:databaseUrl.href,max:1});expect((await restored.query("SELECT value FROM recovery_probe WHERE id='authority'")).rows[0].value).toBe("before-backup");await restored.end();expect(await readFile(join(home,"data/files/holding/evidence/document.txt"),"utf8")).toBe("immutable file before backup\n");expect(await readFile(join(home,"config/production.env"),"utf8")).toBe("BETTER_AUTH_SECRET=before-backup\n");expect(await readlink(join(home,"current"))).toBe(release);
 expect(await readFile(join(home,"data/config/databases.json"),"utf8")).toContain(databaseUrl.href);
 const tampered=join(home,"tampered-snapshot");await cp(snapshotPath,tampered,{recursive:true});await writeFile(join(tampered,"files/holding/evidence/document.txt"),"tampered snapshot\n");await expect(execute("python3",[operator,"--home",home,"--offline-test","restore",tampered],{timeout:120000})).rejects.toBeTruthy();expect(existsSync(join(home,"data/maintenance"))).toBe(false);expect(await readFile(join(home,"data/files/holding/evidence/document.txt"),"utf8")).toBe("immutable file before backup\n");
 const dump=await readFile(join(snapshotPath,"databases",`${database}.dump`)),sha256=createHash("sha256").update(dump).digest("hex"),evidence={kind:"full_offline_backup_restore",database,releaseId,databaseRowsRestored:1,privateFilesRestored:1,applicationSecretRestored:true,databaseManifestRetained:true,archiveSha256:sha256,scope:"Isolated complete operator backup/restore rehearsal; excludes production host, off-machine durability, WAL/PITR and live RPO/RTO sign-off"};
 const evidenceDirectory=join(process.cwd(),".scratch/cross-department-audit");await mkdir(evidenceDirectory,{recursive:true});await writeFile(join(evidenceDirectory,"full-recovery-latest.json"),JSON.stringify(evidence,null,2)+"\n");
},180000);
