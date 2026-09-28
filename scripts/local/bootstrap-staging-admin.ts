import { randomBytes } from 'node:crypto';
import { dirname, resolve, sep } from 'node:path';
import { mkdir, writeFile } from 'node:fs/promises';
import { hashPassword } from 'better-auth/crypto';
import { getRuntime, closeRuntime } from '../../src/server/runtime/env';
import { bootstrapAdmin } from './bootstrap-admin-service';

const base='/Library/Application Support/ERPX-Staging';
const email=process.argv[2]?.trim().toLowerCase();
const credentialsPath=process.env.ERPX_STAGING_CREDENTIALS_PATH;
if(process.env.ERPX_ENVIRONMENT!=='staging'||process.env.ERPX_PUBLIC_BETA!=='false')
  throw new Error('STAGING_ADMIN_BOOTSTRAP_ONLY');
if(!email||!/^\S+@\S+\.\S+$/.test(email))throw new Error('STAGING_ADMIN_EMAIL_REQUIRED');
if(!credentialsPath||!resolve(credentialsPath).endsWith(`${sep}.scratch${sep}erpx-staging${sep}admin-credentials.txt`))
  throw new Error('STAGING_CREDENTIALS_PATH_REQUIRED');
if(resolve(process.env.ERPX_DATA_DIR??'')!==`${base}/data`
  ||!resolve(process.env.ERPX_DATABASE_MANIFEST??'').startsWith(`${base}/data/config/`))
  throw new Error('STAGING_DATABASE_REQUIRED');

const env=getRuntime();
try{
  const existing=await env.CONTROL_DB.prepare('SELECT id FROM "user" WHERE lower(email)=lower(?)').bind(email).first<{id:string}>();
  if(existing)throw new Error('STAGING_ADMIN_ALREADY_EXISTS');
  const id=`usr_stage_${randomBytes(16).toString('hex')}`;
  const password=randomBytes(32).toString('base64url');
  const passwordHash=await hashPassword(password);
  const now=Date.now();
  await env.CONTROL_DB.batch([
    env.CONTROL_DB.prepare('INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,?,?,?)')
      .bind(id,'ERPX Staging Administrator',email,1,now,now),
    env.CONTROL_DB.prepare('INSERT INTO account(id,accountId,providerId,issuer,userId,password,createdAt,updatedAt) VALUES(?,?,?,?,?,?,?,?)')
      .bind(`acc_${id}`,id,'credential','local:credential',id,passwordHash,now,now),
    env.CONTROL_DB.prepare('INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES(?,?,? ,\'active\',?)')
      .bind(`pri_${id}`,id,'ERPX Staging Administrator',now),
  ]);
  process.env.ERPX_ADMIN_EMAIL=email;
  await bootstrapAdmin(env.CONTROL_DB,email,env.H003_DB);
  await mkdir(dirname(credentialsPath),{recursive:true,mode:0o700});
  await writeFile(credentialsPath,`email=${email}\npassword=${password}\n`,{encoding:'utf8',mode:0o600,flag:'wx'});
  console.log(`Staging administrator provisioned; credentials saved to ${credentialsPath}`);
}finally{await closeRuntime();}
