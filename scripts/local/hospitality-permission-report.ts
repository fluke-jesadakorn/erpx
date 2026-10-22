/** Read-only development evidence for the hospitality IAM migration. */
import { readFile, writeFile } from "node:fs/promises";
import { PostgresDatabase } from "@erpx/server-runtime/adapters/runtime/postgres";
import { assertHospitalityDevelopmentTarget } from "@erpx/feature-shared/lib/hospitality-seed-target";
import type { DatabaseManifest } from "@erpx/server-runtime/adapters/runtime/env";
const path=process.env.ERPX_DATABASE_MANIFEST;
if(!path)throw new Error("DEVELOPMENT_MANIFEST_REQUIRED");
const manifest=JSON.parse(await readFile(path,"utf8")) as DatabaseManifest;
assertHospitalityDevelopmentTarget(manifest,process.env.ERPX_ENVIRONMENT);
const reports=[];
const array=(value:unknown):string[]=>typeof value==="string"?JSON.parse(value):value as string[];
for(const [binding,connectionString] of Object.entries(manifest.databases)) {
 if(!/^H\d+_DB$/.test(binding))continue;
 const db=new PostgresDatabase({connectionString,max:1});
 try {
  const changes=(await db.prepare("SELECT company_id,role_id,before_revision,after_revision,before_permissions,after_permissions FROM hospitality_permission_changes ORDER BY company_id,role_id").all<{company_id:string;role_id:string;before_revision:number;after_revision:number;before_permissions:unknown;after_permissions:unknown}>()).results.map(row=>{
   const before=array(row.before_permissions).sort(),after=array(row.after_permissions).sort();
   return {companyId:row.company_id,roleId:row.role_id,beforeRevision:row.before_revision,afterRevision:row.after_revision,before,after,removed:before.filter(p=>!after.includes(p)),added:after.filter(p=>!before.includes(p))};
  });
  reports.push({binding,changes});
 }finally{await db.close();}
}
const output=process.argv[2]??".scratch/dev/hospitality-permission-report.json";
await writeFile(output,JSON.stringify({environment:"development",reports},null,2)+"\n",{mode:0o600});
console.log(JSON.stringify({output,holdings:reports.length,roleChanges:reports.reduce((sum,r)=>sum+r.changes.length,0)}));
