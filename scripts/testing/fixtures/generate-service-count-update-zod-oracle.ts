import {writeFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {countSchema} from '../../../packages/server-inventory/src/domain/counts';
const base=(extra='')=>`{"warehouseId":"w","countDate":"2026-09-01","reason":"  first count  ",${extra}"lines":[{"productId":"p","countedQuantity":"2.00"}]}`;
const cases:[string,string][]=[
 ['defaults',base()],
 ['integer-float',base('"expectedVersion":1.0,')],
 ['duplicate',base('"expectedVersion":2,"expectedVersion":3,')],
 ['trim-feff',base().replace('  first count  ','\\ufeff First count \\ufeff')],
 ['preserve-nel',base().replace('  first count  ','\\u0085First count\\u0085')],
 ['surrogate',base().replace('"productId":"p"','"productId":"\\ud800"')],
 ['unknown',base().replace('"productId":"p"','"ignored":true,"productId":"p"').replace('"warehouseId"','"unknown":{},"warehouseId"')],
 ['unknown-surrogate-root',base().replace('"warehouseId"','"\\ud800":1,"warehouseId"')],
 ['unknown-surrogate-line',base().replace('"productId"','"\\ud800":1,"productId"')],
 ['unknown-surrogate-lot',base().replace('"countedQuantity":"2.00"','"countedQuantity":"2.00","allocations":[{"\\ud800":1,"lotCode":" A ","countedQuantity":"2.00","unitCost":"1.50"}]')],
 ['nested-lots',base().replace('"countedQuantity":"2.00"','"countedQuantity":"2.00","allocations":[{"lotCode":" A ","countedQuantity":"2.00","unitCost":"1.50","ignored":true}]')],
 ['two-lines',base().replace('}]','},{"productId":"p2","countedQuantity":"0"}]')],
 ['invalid-date',base().replace('2026-09-01','2026-02-30')],
 ['invalid-decimal',base().replace('2.00','2.0001')],
 ['invalid-null',base('"expectedVersion":null,')],
 ['unsafe-version',base('"expectedVersion":9007199254740992,')],
 ['empty-lines',base().replace('[{"productId":"p","countedQuantity":"2.00"}]','[]')],
];
const output=cases.map(([name,raw])=>{const result=countSchema.safeParse(JSON.parse(raw));return{name,raw,normalized:result.success?JSON.stringify(result.data):null};});
writeFileSync(resolve(import.meta.dir,'../../../fixtures/field_service/service-count-update-zod-oracle.v1.json'),`${JSON.stringify({version:1,cases:output},null,2)}\n`);
