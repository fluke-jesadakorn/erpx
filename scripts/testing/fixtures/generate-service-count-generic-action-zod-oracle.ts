import {writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {countActionSchema} from '../../../packages/server-inventory/src/domain/counts';
const cases:[string,string][]=[
 ['defaults','{"expectedVersion":1}'],
 ['trim','{"reason":"\\ufeff Reviewed \\ufeff","expectedVersion":3}'],
 ['nested-unknown','{"expectedVersion":2,"unknown":{"nested":[1,{"x":true}]},"attachmentSetIds":["set-1"]}'],
 ['surrogate-key','{"\\ud800":{"nested":true},"expectedVersion":2}'],
 ['nested-surrogate-key','{"expectedVersion":2,"ignored":{"\\ud800":1}}'],
 ['duplicate-version','{"expectedVersion":1,"expectedVersion":3}'],
 ['float-integer','{"expectedVersion":2.0}'],
 ['invalid-null','{"expectedVersion":null}'],
 ['zero-cas','{"expectedVersion":0}'],
 ['unsafe-cas','{"expectedVersion":9007199254740992}'],
 ['short-reason','{"expectedVersion":1,"reason":"  x  "}'],
 ['bad-ids','{"expectedVersion":1,"attachmentSetIds":[""]}'],
 ['too-many-ids',JSON.stringify({expectedVersion:1,attachmentSetIds:Array(11).fill('set')})],
 ['nonobject','[]'],
 ['bad-json','{'],
];
const output=cases.map(([name,raw])=>{try{const result=countActionSchema.safeParse(JSON.parse(raw));return{name,raw,normalized:result.success?JSON.stringify(result.data):null};}catch{return{name,raw,normalized:null};}});
writeFileSync(resolve(import.meta.dir,'../../../fixtures/field_service/service-count-generic-action-zod-oracle.v1.json'),`${JSON.stringify({version:1,cases:output},null,2)}\n`);
