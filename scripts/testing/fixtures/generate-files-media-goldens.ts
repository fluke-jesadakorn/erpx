/** Development oracle: execute unchanged B0 owner rules on pinned Node. */
import {createHash} from "node:crypto";
import {execFileSync} from "node:child_process";
import {mkdir,readFile,writeFile} from "node:fs/promises";
import {resolve} from "node:path";

const sourceCommit="a5ed8737a2f05eff1ee9841fda4a094b3926d732";
const sourcePaths=["packages/server-files/src/domain/upload-validation.ts","packages/server-files/src/domain/attachments/media-signature.ts"];
const sources=[];
for(const path of sourcePaths){
  const bytes=await readFile(path),prior=execFileSync("git",["show",`${sourceCommit}:${path}`]);
  if(!bytes.equals(prior))throw new Error("FILES_ORACLE_SOURCE_CHANGED");
  sources.push({path,sha256:createHash("sha256").update(bytes).digest("hex")});
}
const node=process.env.ERPX_NODE_BIN??"node";
if(execFileSync(node,["--version"],{encoding:"utf8"}).trim()!=="v26.11.1")throw new Error("PINNED_FILES_ORACLE_NODE_REQUIRED");
const directory=resolve(".scratch/dev/files-rust-oracle");await mkdir(directory,{recursive:true,mode:0o700});
const entry=resolve(directory,"entry.ts");
await writeFile(entry,`
import {matchesUploadSignature} from ${JSON.stringify(resolve(sourcePaths[0]!))};
import {detectedAttachmentMediaType} from ${JSON.stringify(resolve(sourcePaths[1]!))};
import {readFileSync} from "node:fs";
const input=JSON.parse(readFileSync(0,"utf8"));
process.stdout.write(JSON.stringify({node:process.version,cases:input.map(row=>({...row,detected:detectedAttachmentMediaType(Buffer.from(row.hex,"hex"),row.declared),matches:matchesUploadSignature(row.declared,Buffer.from(row.hex,"hex"))}))}));
`,{mode:0o600});
const build=await Bun.build({entrypoints:[entry],outdir:directory,target:"node",format:"esm",naming:"oracle.mjs"});
if(!build.success)throw new Error("FILES_NODE_ORACLE_BUILD_FAILED");
const declared=["application/pdf","image/png","image/jpeg","image/webp","image/heic","image/heif","application/vnd.openxmlformats-officedocument.wordprocessingml.document","text/csv","application/octet-stream"];
const be=(value:number)=>{const bytes=Buffer.alloc(4);bytes.writeUInt32BE(value);return bytes;};
const box=(type:string,payload:Buffer)=>Buffer.concat([be(8+payload.length),Buffer.from(type),payload]);
const heif=(brand:string)=>Buffer.concat([box("ftyp",Buffer.concat([Buffer.from(brand),be(0),Buffer.from("mif1")])),box("meta",Buffer.alloc(4))]);
const central=(name:string)=>{const n=Buffer.from(name),h=Buffer.alloc(46);h.writeUInt32LE(0x02014b50);h.writeUInt16LE(n.length,28);return Buffer.concat([h,n]);};
const centralBytes=Buffer.concat([central("[Content_Types].xml"),central("word/document.xml")]);
const zipPrefix=Buffer.from([0x50,0x4b,3,4]),end=Buffer.alloc(22);end.writeUInt32LE(0x06054b50);end.writeUInt32LE(centralBytes.length,12);end.writeUInt32LE(zipPrefix.length,16);
const zip=Buffer.concat([zipPrefix,centralBytes,end]);
const png=Buffer.concat([Buffer.from([0x89,0x50,0x4e,0x47]),Buffer.alloc(8),Buffer.from([0,0,0,0,0x49,0x45,0x4e,0x44,0xae,0x42,0x60,0x82])]);
const webp=Buffer.from("RIFF\x04\x00\x00\x00WEBP","latin1");
const seeds=[Buffer.alloc(0),Buffer.from("%PDF-1.7\nsynthetic\n%%EOF\n"),Buffer.from("%PDF-1.7\n%%EOF\nTRAIL"),png,Buffer.from([0xff,0xd8,0xff,0x00,0xff,0xd9,0]),webp,heif("heic"),heif("mif1"),heif("avif"),zip,Buffer.from("name,value\nsynthetic,1\n"),Buffer.from("ชื่อ,จำนวน\nตัวอย่าง,1\n"),Buffer.from([0xc3,0x28]),Buffer.from([0,0xff])];
const inputs=new Map<string,Buffer>();
function add(bytes:Buffer){inputs.set(bytes.toString("hex"),bytes);}
for(const seed of seeds){
  add(seed);
  for(let len=0;len<=Math.min(seed.length,40);len++)add(seed.subarray(0,len));
  for(const suffix of [Buffer.from([0,9,10,12,13,32]),Buffer.from("unexpected")])add(Buffer.concat([seed,suffix]));
  for(const at of [0,3,7,11,seed.length-1])if(at>=0&&at<seed.length){const changed=Buffer.from(seed);changed[at]=(changed[at]??0)^0xff;add(changed);}
}
// Fixed LCG adds malformed binary cases without depending on host randomness.
let state=0x45_52_50_58;
for(let index=0;index<80;index++){const bytes=Buffer.alloc(index%33);for(let at=0;at<bytes.length;at++){state=(Math.imul(state,1664525)+1013904223)>>>0;bytes[at]=state>>>24;}add(bytes);}
const cases=[...inputs.keys()].flatMap((hex,index)=>declared.map(type=>({name:`bytes-${index}:${type}`,hex,declared:type})));
const oracle=JSON.parse(execFileSync(node,[resolve(directory,"oracle.mjs")],{input:JSON.stringify(cases),encoding:"utf8",maxBuffer:16*1024*1024}));
await mkdir("fixtures/files/rust-media",{recursive:true});
await writeFile("fixtures/files/rust-media/conformance.v1.json",JSON.stringify({version:1,sourceCommit,sources,node:oracle.node,scope:"Byte-signature policy only; synthetic containers do not certify decoder validity or clean scanning.",cases:oracle.cases},null,2)+"\n");
console.log(`Files media oracle: ${oracle.cases.length} unchanged-source Node cases.`);
