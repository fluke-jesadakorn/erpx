/** Independent dev-only oracle: exact retained route wire schemas, no authentication or DB acceptance. */
import {execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {writeFile,readFile,mkdir} from 'node:fs/promises';
import {resolve} from 'node:path';
const commit='a5ed8737a2f05eff1ee9841fda4a094b3926d732';
const paths=['packages/workplace/src/routes/api/v1/holdings/[holdingSlug]/finance/[...path]/route.ts','packages/feature-legal/src/routes/api/v1/holdings/[holdingSlug]/legal/[[...path]]/route.ts'];
const bytes=paths.map(path=>execFileSync('git',['show',`${commit}:${path}`]));
const finance=bytes[0]!.toString('utf8'),legal=bytes[1]!.toString('utf8');
function schema(kind:string){const start=finance.indexOf(`parts[2] === "${kind}") { const parsed = `);if(start<0)throw Error('ORIGINAL_WIRE_SCHEMA_BOUNDARY_REQUIRED');const a=finance.indexOf('const parsed = ',start)+'const parsed = '.length,b=finance.indexOf('.safeParse(input)',a);if(b<a)throw Error('ORIGINAL_WIRE_SCHEMA_BOUNDARY_REQUIRED');return finance.slice(a,b);}
const bodyStart=finance.indexOf('async function body(request: Request)'),bodyEnd=finance.indexOf('\nfunction needCommand',bodyStart);
if(bodyStart<0||bodyEnd<bodyStart)throw Error('ORIGINAL_BODY_BOUNDARY_REQUIRED');
const legalStart=legal.indexOf('    if (\n      path.length === 3 &&\n      path[0] === "documents"'),legalEnd=legal.indexOf('    if (path.length && path[0] !== "release-acceptance")',legalStart);
if(legalStart<0||legalEnd<legalStart)throw Error('ORIGINAL_LEGAL_QUEUE_BOUNDARY_REQUIRED');
const dir=resolve('.scratch/dev/native-business-http/oracle');await mkdir(dir,{recursive:true,mode:0o700});
const entry=resolve(dir,'entry.ts');
await writeFile(entry,`import {z} from 'zod';import {readFileSync} from 'node:fs';
class ApiError extends Error{constructor(status,code,message){super(message);this.status=status;this.code=code;}}
${finance.slice(bodyStart,bodyEnd)}
const post=${schema('post')},reverse=${schema('reverse')};
const input=JSON.parse(readFileSync(0,'utf8'));const results=[];
for(const row of input.cases){try{const request=new Request('https://acme.example.test/api',{method:'POST',body:Buffer.from(row.base64,'base64'),headers:row.contentType===null?{}:{'content-type':row.contentType}});const data=await body(request);const parsed=(row.kind==='post'?post:reverse).safeParse(data);if(!parsed.success)throw new ApiError(422,'VALIDATION_FAILED',row.kind==='post'?'expectedVersion is required.':'A reversal period and date are required.');results.push({...row,expected:{status:row.kind==='post'?200:201,normalized:JSON.stringify(parsed.data)}});}catch(error){results.push({...row,expected:{status:error.status,code:error.code,message:error.message}});}}
async function legalWire(path,key,request){const queueInspection=async(_write,_a,documentId,kind,id)=>({id:'item',jobId:id,state:'queued'});const canonicalDestinationLinks=value=>value;const write={},a={};${legal.slice(legalStart,legalEnd)}throw Error('unmatched');}
const legalResults=[];for(const row of input.legal){const request=new Request('https://acme.example.test/api',{method:'POST',body:row.body,headers:row.contentType===null?{}:{'content-type':row.contentType}});const result=await legalWire(['documents','document',row.kind],'key',request);legalResults.push({...row,expected:{status:result.status,body:await result.json()}});}
process.stdout.write(JSON.stringify({node:process.version,cases:results,legal:legalResults}));
`,{mode:0o600});
const build=await Bun.build({entrypoints:[entry],outdir:dir,naming:'oracle.mjs',target:'node',format:'esm'});if(!build.success)throw Error(build.logs.join('\n'));
const cases:{id:string;kind:string;base64:string;contentType:string|null}[]=[];function add(id:string,kind:string,raw:string,contentType:string|null='application/json'){cases.push({id,kind,base64:Buffer.from(raw).toString('base64'),contentType});}
for(const number of ['1','1.0','1e0','1.0000000000000001','0','-0','-1','1.5','1e400','9007199254740991','9007199254740992','"1"','true','null'])add('post-number-'+number,'post',`{"expectedVersion":${number}}`);
for(const [id,raw]of Object.entries({missing:'{}',primitive:'1',array:'[]',null:'null',duplicate:'{"expectedVersion":1,"expectedVersion":2}',duplicateInvalidFirst:'{"expectedVersion":null,"expectedVersion":1}',duplicateInvalidLast:'{"expectedVersion":1,"expectedVersion":null}',unknown:'{"expectedVersion":1,"ignored":{"negative":1e400,"unpaired":"\\ud800"}}',empty:'',malformed:'{"expectedVersion":',bom:'\ufeff{"expectedVersion":1}'}))add('post-'+id,'post',raw);
for(const contentType of [null,'text/plain','application/json; charset=utf-8'])add('post-content-'+contentType,'post','{"expectedVersion":1}',contentType);
for(const [id,memo]of Object.entries({absent:undefined,empty:'""',null:'null',false:'false',number:'0',thai:JSON.stringify('ข้อความ 😀'),unpaired:'"\\ud800"',pair:'"\\ud83d\\ude00"',newline:'"first\\nsecond"'}))add('reverse-memo-'+id,'reverse',`{"periodId":"period","entryDate":"2026-01-01"${memo===undefined?'':`,"memo":${memo}`}}`);
for(const [id,raw]of Object.entries({emptyPeriod:'{"periodId":"","entryDate":"2026-01-01"}',missingPeriod:'{"entryDate":"2026-01-01"}',dateNull:'{"periodId":"period","entryDate":null}',wrongDate:'{"periodId":"period","entryDate":"2026-02-29"}',leapDate:'{"periodId":"period","entryDate":"2024-02-29"}',datePadding:'{"periodId":"period","entryDate":"2026-1-01"}',unknown:'{"periodId":"period","entryDate":"2026-01-01","ignored":null}',duplicate:'{"periodId":"period","entryDate":"2026-01-01","memo":"old","memo":""}'}))add('reverse-'+id,'reverse',raw);
const legalCases=['ocr','verify'].flatMap(kind=>[null,'application/json','text/plain'].flatMap(contentType=>['','not-json','null'].map(body=>({kind,contentType,body}))));
const node=process.env.ERPX_NODE_BIN??'node';if(execFileSync(node,['--version'],{encoding:'utf8'}).trim()!=='v26.11.1')throw Error('PINNED_NODE_REQUIRED');
const output=JSON.parse(execFileSync(node,[resolve(dir,'oracle.mjs')],{input:JSON.stringify({cases,legal:legalCases}),encoding:'utf8',maxBuffer:4*1024*1024,env:{PATH:process.env.PATH??'',LANG:'en_US.UTF-8',TZ:'UTC',NODE_ENV:'test'}}));
const document=JSON.stringify({version:1,commit,sources:paths.map((path,i)=>({path,sha256:createHash('sha256').update(bytes[i]!).digest('hex')})),scope:'Exact retained HTTP wire schemas/body/status only. Legal queue result is a synthetic transport port; no auth, current-source or database acceptance.',...output},null,2)+'\n';
const destination=resolve('fixtures/finance/rust-journal-wire/conformance.v1.json');if(process.argv.includes('--check')){if(await readFile(destination,'utf8')!==document)throw Error('WIRE_ORACLE_DRIFT');}else await writeFile(destination,document,{mode:0o600});
console.log(JSON.stringify({finance:output.cases.length,legal:output.legal.length,node:output.node,check:process.argv.includes('--check')}));
