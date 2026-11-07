import {readFile,writeFile} from 'node:fs/promises';
for(const owner of ['field_service','operations','collaboration','files','finance']){
 const root=`packages/server-${owner}/src/public/`;
 const source=await readFile(root+'field_outbox_quarantine.sql','utf8');
 const output=`// Generated from owner field_outbox_quarantine.sql; scripts/docs/generate-field-outbox-predicates.ts\nexport const fieldOutboxQuarantine = ${JSON.stringify(source)};\n`;
 const path=root+'field-outbox-predicate.generated.ts';
 if(process.argv.includes('--check')){if(await readFile(path,'utf8')!==output)throw Error('FIELD_OUTBOX_PREDICATE_DRIFT');}else await writeFile(path,output);
}

// The migration is immutable after delivery. Fail on later owner-policy drift;
// a changed runtime ownership policy needs a new reviewed migration.
const migration=await readFile('migrations/postgres/holding-write/1097_native_field_journal_flow.sql','utf8');
for(const path of ['packages/server-inventory/src/public/service_count_outbox_quarantine.sql',...['field_service','operations','collaboration','files','finance'].map(owner=>`packages/server-${owner}/src/public/field_outbox_quarantine.sql`)]){
 if(!migration.includes('('+((await readFile(path,'utf8')).trim())+')'))throw Error('NATIVE_OUTBOX_DATABASE_POLICY_DRIFT');
}
