/** Private native leases for the already-provisioned, keyed contributor cluster. */
import { randomBytes } from 'node:crypto';
import { chmodSync, chownSync, closeSync, existsSync, lstatSync, mkdirSync, openSync, readFileSync, renameSync, statSync, unlinkSync, writeFileSync } from 'node:fs';
import { Pool } from 'pg';

const bindings = ['CONTROL_DB', 'CONTROL_READ_DB', ...['H001', 'H002', 'H003'].flatMap(id => [`${id}_DB`, `${id}_READ_DB`])];
const files = [['databases.json', 'app'], ['databases-jobs.json', 'jobs'], ['databases-migrations.json', 'migration']] as const;
type Entry = { binding: string; oldName: string; newName: string; token: string };
type Leases = { version: 1; key: string; sourceId: string; password: string; entries: Entry[]; complete: boolean };
const ident = (name: string) => { if (!/^[a-z][a-z0-9_]{2,62}$/.test(name)) throw Error('NATIVE_IDENTIFIER_INVALID'); return `"${name}"`; };
function oldName(binding: string) { return `erpx_dev_${binding.toLowerCase().replace(/_db$/, '').replace(/^(h00[1-3])$/, '$1_write')}`; }
function privateRead(path: string) { if (!existsSync(path) || (statSync(path).mode & 0o077) !== 0) throw Error('NATIVE_PRIVATE_FILE_REQUIRED'); return readFileSync(path, 'utf8'); }
function writeAtomic(path: string, value: unknown) {
  const tmp = `${path}.${randomBytes(16).toString('hex')}.tmp`;
  const fd = openSync(tmp, 'wx', 0o600);
  let renamed = false;
  try { writeFileSync(fd, JSON.stringify(value)); closeSync(fd); renameSync(tmp, path); renamed = true; }
  finally { if (!renamed) { try { closeSync(fd); } catch { /* already closed */ } unlinkSync(tmp); } }
}
export async function prepareNativeLeases(config: string, key: string, sourceId: string, adminUrl: string) {
  const path = `${config}/native-leases.json`, pool = new Pool({ connectionString: adminUrl, max: 1 });
  try {
    const mountedPassword = privateRead('/run/erpx-secrets/test-admin').trim();
    if (!/^[a-f0-9]{64}$/.test(mountedPassword)) throw Error('NATIVE_ADMIN_SECRET_INVALID');
    const loginUrl = new URL('postgresql://erpx_test_admin@127.0.0.1:5432/postgres'); loginUrl.password = mountedPassword;
    const login = new Pool({ connectionString: loginUrl.href, max: 1 });
    try { const identity = await login.query('SELECT current_user::text role,session_user::text session_role'); if (identity.rows[0]?.role !== 'erpx_test_admin' || identity.rows[0]?.session_role !== 'erpx_test_admin') throw Error('NATIVE_ADMIN_LOGIN_INVALID'); } finally { await login.end(); }
    let state: Leases;
    if (existsSync(path)) state = JSON.parse(privateRead(path)) as Leases;
    else {
      state = { version: 1, key, sourceId, password: mountedPassword, complete: false, entries: bindings.map(binding => { const token=randomBytes(16).toString('hex'); return { binding, oldName: oldName(binding), newName: `erpx_dev_rust_${token}`, token }; }) };
      writeAtomic(path, state);
    }
    if (state.version !== 1 || state.key !== key || state.sourceId !== sourceId || state.password !== mountedPassword || typeof state.complete !== 'boolean' || !Array.isArray(state.entries) || state.entries.length !== bindings.length || new Set(state.entries.map(e => e.token)).size !== bindings.length) throw Error('NATIVE_LEASE_MANIFEST_INVALID');
    for (let i=0; i<bindings.length; i++) { const e=state.entries[i]; if (!e || e.binding!==bindings[i] || e.oldName!==oldName(bindings[i]!) || !/^[a-f0-9]{32}$/.test(e.token) || e.newName!==`erpx_dev_rust_${e.token}`) throw Error('NATIVE_LEASE_MANIFEST_INVALID'); }
    // Check all identities and active clients before the first rename. RENAME itself
    // remains the final race-safe check and PostgreSQL rejects active sessions.
    for (const e of state.entries) {
      const check = await pool.query<{datname:string;owner:string;lease:string|null;active:number}>("SELECT d.datname,pg_get_userbyid(d.datdba) owner,shobj_description(d.oid,'pg_database') lease,(SELECT count(*)::int FROM pg_stat_activity a WHERE a.datid=d.oid) active FROM pg_database d WHERE d.datname=ANY($1::text[])", [[e.oldName,e.newName]]);
      if (check.rowCount!==1) throw Error('NATIVE_DATABASE_IDENTITY_INVALID');
      const row=check.rows[0]!, expected=`erpx-rust-test:${e.token}`, migration=`${e.oldName}_migration`;
      if (row.datname===e.oldName ? state.complete || row.owner!==migration || row.lease!==null || row.active!==0 : ![migration,'erpx_test_admin'].includes(row.owner) || ![null,expected].includes(row.lease) || (state.complete && (row.owner!=='erpx_test_admin' || row.lease!==expected))) throw Error('NATIVE_DATABASE_IDENTITY_INVALID');
    }
    const role = await pool.query<{rolsuper:boolean;rolcreaterole:boolean;rolcreatedb:boolean;rolcanlogin:boolean}>("SELECT rolsuper,rolcreaterole,rolcreatedb,rolcanlogin FROM pg_roles WHERE rolname='erpx_test_admin'");
    if (role.rowCount !== 1 || role.rows[0]!.rolsuper || role.rows[0]!.rolcreaterole || !role.rows[0]!.rolcreatedb || !role.rows[0]!.rolcanlogin) throw Error('NATIVE_ADMIN_INVALID');
    for (const e of state.entries) {
      const rows=await pool.query<{datname:string;owner:string;lease:string|null}>("SELECT d.datname,pg_get_userbyid(d.datdba) owner,shobj_description(d.oid,'pg_database') lease FROM pg_database d WHERE d.datname=ANY($1::text[])", [[e.oldName,e.newName]]);
      if (rows.rowCount!==1) throw Error('NATIVE_DATABASE_IDENTITY_INVALID');
      const row=rows.rows[0]!, migration=`${e.oldName}_migration`, expected=`erpx-rust-test:${e.token}`;
      if (row.datname===e.oldName) {
        if (state.complete || row.owner!==migration || row.lease!==null) throw Error('NATIVE_DATABASE_IDENTITY_INVALID');
        await pool.query(`ALTER DATABASE ${ident(e.oldName)} RENAME TO ${ident(e.newName)}`);
      } else if (![migration,'erpx_test_admin'].includes(row.owner) || ![null,expected].includes(row.lease) || (state.complete && (row.owner!=='erpx_test_admin' || row.lease!==expected))) throw Error('NATIVE_DATABASE_IDENTITY_INVALID');
      if (!state.complete) {
        await pool.query(`GRANT ${ident(migration)} TO erpx_test_admin`);
        await pool.query(`ALTER DATABASE ${ident(e.newName)} OWNER TO erpx_test_admin`);
        await pool.query(`COMMENT ON DATABASE ${ident(e.newName)} IS '${expected}'`);
      } else {
        const membership=await pool.query('SELECT pg_has_role($1,$2,\'MEMBER\') AS ok',['erpx_test_admin',migration]); if (!membership.rows[0]?.ok) throw Error('NATIVE_ADMIN_MEMBERSHIP_INVALID');
      }
      // The old migration owner loses its implicit CONNECT when ownership moves.
      // Restore only that database's existing migration role; never grant PUBLIC.
      const access=await pool.query("SELECT has_database_privilege($1,$2,'CONNECT') AS ok",[migration,e.newName]);
      if (!access.rows[0]?.ok) await pool.query(`GRANT CONNECT ON DATABASE ${ident(e.newName)} TO ${ident(migration)}`);
    }
    const roles=JSON.parse(privateRead(`${config}/provisioning.json`)) as Record<string,string>;
    for (const [file, kind] of files) {
      const p=`${config}/${file}`, manifest=JSON.parse(privateRead(p)) as { databases: Record<string,string> };
      if (!manifest.databases || Object.keys(manifest.databases).length!==bindings.length) throw Error('NATIVE_SOURCE_MANIFEST_INVALID');
      for (const e of state.entries) { const url=new URL(manifest.databases[e.binding]!); const roleName=`${e.oldName}_${kind}`;
        if (!['postgres:','postgresql:'].includes(url.protocol)||url.hostname!=='127.0.0.1'||url.port!=='5432'||url.search||url.hash||url.username!==roleName||url.password!==roles[roleName]||!/^[a-f0-9]{64}$/.test(roles[roleName]??'')||![`/${e.oldName}`,`/${e.newName}`].includes(url.pathname)) throw Error('NATIVE_SOURCE_MANIFEST_INVALID');
        if (state.complete && url.pathname!==`/${e.newName}`) throw Error('NATIVE_SOURCE_MANIFEST_INVALID');
        url.pathname=e.newName; manifest.databases[e.binding]=url.href;
      }
      if (!state.complete) writeAtomic(p, manifest);
    }
    if (!state.complete) { state.complete=true; writeAtomic(path,state); }
    return state;
  } finally { await pool.end(); }
}

/** Configs are private; the caller decides how to deliver them to non-root containers. */
export async function writeNativeConfiguration(config: string, state: Leases) {
  const byBinding = new Map(state.entries.map(entry => [entry.binding, entry]));
  function database(binding: string) {
    const entry=byBinding.get(binding); if (!entry) throw Error('NATIVE_BINDING_REQUIRED');
    const url=new URL(`postgresql://erpx_test_admin@127.0.0.1:5432/${entry.newName}`); url.password=state.password;
    return { url:url.href, leaseToken:entry.token };
  }
  // Verify login and the lease via the same authority Rust requires before emitting config.
  for (const entry of state.entries) {
    const pool=new Pool({connectionString:database(entry.binding).url,max:1});
    try { const result=await pool.query("SELECT current_database() name,current_user::text role,session_user::text session_role,pg_get_userbyid(d.datdba)::text owner,shobj_description(d.oid,'pg_database') lease,r.rolsuper,r.rolcreaterole FROM pg_database d JOIN pg_roles r ON r.rolname=current_user WHERE d.datname=current_database()");
      const row=result.rows[0]; if (!row||row.name!==entry.newName||row.role!=='erpx_test_admin'||row.session_role!=='erpx_test_admin'||row.owner!=='erpx_test_admin'||row.lease!==`erpx-rust-test:${entry.token}`||row.rolsuper||row.rolcreaterole) throw Error('NATIVE_LEASE_CHECK_FAILED');
    } finally { await pool.end(); }
  }
  const holdings=[['h001','acme'],['h002','northstar'],['h003','erpx']].map(([id,slug],i)=>({holdingId:id,slug,host:`${id === 'h003' ? 'erpx-internal' : slug}.erpx.localhost:3443`,writeBinding:`H00${i+1}_DB`,readBinding:`H00${i+1}_READ_DB`,filesBinding:`H00${i+1}_FILES`,database:database(`H00${i+1}_DB`),readDatabase:database(`H00${i+1}_READ_DB`)}));
  const filesRoot='/workspace/erpx-native-files', temporaryRoot='/run/erpx-native-files-tmp';
  for (const path of [filesRoot,temporaryRoot]) { mkdirSync(path,{recursive:true,mode:0o700}); const info=lstatSync(path); if(!info.isDirectory()||info.isSymbolicLink())throw Error('NATIVE_FILES_DIRECTORY_INVALID'); chownSync(path,65532,65532); chmodSync(path,0o700); }
  const document={version:1,mode:'development',transport:'direct-loopback',listen:'127.0.0.1:44817',accountHost:'account.erpx.localhost:3443',requireEmailVerification:true,passwordWorkers:2,maximumRequests:16,requestTimeoutMs:5000,control:database('CONTROL_DB'),holdings,fieldServiceReads:[{holdingId:'h001',companyId:'acme-co'}],fieldServicePartsPostingSetups:['h001'],fieldServicePartsPrerequisites:['h001'],fieldServiceStockSetups:['h001'],fieldServiceWorkspaceReads:['h001'],fieldServiceDetailReads:['h001'],fieldAttachmentLinks:[{holdingId:'h001',mode:'enabled'}],fieldAttachmentBinds:[{holdingId:'h001',mode:'enabled'}],fieldFilesPolicy:{mode:'enabled',uploadsFrozen:false},fieldFilesProviders:{targets:[{holdingId:'h001',companyId:'acme-co',filesBinding:'H001_FILES',filesRoot,temporaryRoot,previewBinary:'/run/erpx-native/bin/erpx-render-preview',heifConvert:'/usr/bin/heif-convert',pdftoppm:'/usr/bin/pdftoppm',clamavSocket:'/run/erpx-scanner/clamd.sock',clamavDatabaseDirectory:'/run/erpx-clamav-definitions',maximumDefinitionAgeMs:86400000,commandTimeoutMs:5000,scanTimeoutMs:30000,concurrentScans:1}]},fieldServiceInitialCounts:[{holdingId:'h001',filesEnabled:true,filesReadOnly:false,uploadsFrozen:false}],accountBootstrap:true};
  writeAtomic(`${config}/private-api.json`,document);
  writeAtomic(`${config}/worker.json`,document);
  writeAtomic(`${config}/dev-container-credentials.json`, { maker:{email:'mali@erpx.dev',password:'Mali-ERPX-2026!'}, reviewer:{email:'nisa@erpx.dev',password:'Nisa-ERPX-2026!'} });
}

/** Upgrade only named, synthetic seed accounts; never select arbitrary users. */
export async function prepareSyntheticCredentials(state: Leases, seedPending: boolean) {
  const entry=state.entries.find(e=>e.binding==='CONTROL_DB'); if (!entry) throw Error('NATIVE_CONTROL_REQUIRED');
  const url=new URL(`postgresql://erpx_test_admin@127.0.0.1:5432/${entry.newName}`); url.password=state.password;
  const pool=new Pool({connectionString:url.href,max:1});
  try {
    for (const [id,email,password] of [['usr_mali','mali@erpx.dev','Mali-ERPX-2026!'],['usr_nisa','nisa@erpx.dev','Nisa-ERPX-2026!']] as const) {
      const result=await pool.query('SELECT a.password FROM account a JOIN "user" u ON u.id=a."userId" WHERE a.id=$1 AND a."accountId"=$2 AND a."userId"=$2 AND a."providerId"=\'credential\' AND a.issuer=\'local:credential\' AND u.email=$3',[`acc_${id}`,id,email]);
      if (result.rowCount!==1 || typeof result.rows[0]?.password!=='string') throw Error('NATIVE_SYNTHETIC_ACCOUNT_REQUIRED');
      const current=result.rows[0].password as string;
      if (current.startsWith('$argon2id$v=19$m=65536,t=3,p=1$') && await Bun.password.verify(password,current)) continue;
      if (!seedPending) throw Error('NATIVE_SYNTHETIC_CREDENTIAL_NOT_SEEDED');
      const hash=await Bun.password.hash(password,{algorithm:'argon2id',memoryCost:65536,timeCost:3});
      if (!hash.startsWith('$argon2id$v=19$m=65536,t=3,p=1$')) throw Error('NATIVE_SYNTHETIC_HASH_INVALID');
      const update=await pool.query('UPDATE account SET password=$1 WHERE id=$2 AND "userId"=$3 AND "accountId"=$3 AND "providerId"=\'credential\' AND issuer=\'local:credential\' AND password=$4',[hash,`acc_${id}`,id,current]);
      if (update.rowCount!==1) throw Error('NATIVE_SYNTHETIC_ACCOUNT_CHANGED');
    }
  } finally { await pool.end(); }
}
