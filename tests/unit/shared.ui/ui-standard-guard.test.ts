import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, unlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

const roots: string[] = [];
const fingerprint = (text: string) => createHash("sha256").update(text).digest("hex");

/** Run the real guard in an isolated repository, without mutating live sources or inventories. */
function fixture({
  content = 'import { Button as Action } from "@/components/ui/button"; export const View = () => <Action appearance="standard">Open</Action>;',
  frozenChanged = false,
  boundary = false,
  stale = false,
  complete = false,
  retirement = "none",
} = {}) {
  const root = mkdtempSync(join(tmpdir(), "erpx-ui-guard-"));
  roots.push(root);
  for (const directory of ["scripts/ui", "docs/ux-ui", "src", "node_modules"]) mkdirSync(join(root, directory), { recursive: true });
  copyFileSync(resolve("scripts/ui/check-standard.mjs"), join(root, "scripts/ui/check-standard.mjs"));
  symlinkSync(resolve("node_modules/typescript"), join(root, "node_modules/typescript"), "dir");
  writeFileSync(join(root, "src/view.tsx"), content);
  writeFileSync(join(root, "src/specialized.tsx"), frozenChanged ? "changed layout" : "retained layout");
  writeFileSync(join(root, "scripts/ui/frozen-boundaries.json"), JSON.stringify([
    { source: "src/specialized.tsx", fingerprint: fingerprint("retained layout"), reason: "frozen-specialized" },
  ]));
  writeFileSync(join(root, "docs/ux-ui/standard-coverage.json"), JSON.stringify({
    boundaryConsumers: boundary ? [{ owner: "src/specialized.tsx", component: "src/view.tsx" }] : [],
    routes: [{ verification: "pending" }],
    sources: [{ source: "src/view.tsx", scope: "standardize", presentation: "shared-opt-in-present", fingerprint: fingerprint(stale ? "old source" : content), verification: "pending" }],
  }));
  if (retirement !== "none") {
    const git = (...args: string[]) => {
      const result = spawnSync("git", args, { cwd: root, encoding: "utf8" });
      if (result.status !== 0) throw new Error(result.stderr);
      return result.stdout.trim();
    };
    git("init", "--quiet");
    git("add", "src/specialized.tsx");
    git("-c", "user.name=Guard fixture", "-c", "user.email=guard@example.invalid", "commit", "--quiet", "-m", "Frozen evidence");
    const originalCommit = git("rev-parse", "HEAD");
    unlinkSync(join(root, "src/specialized.tsx"));
    writeFileSync(join(root, "src/successor.tsx"), retirement === "changed" ? "unreviewed layout" : "reviewed successor");
    if (retirement !== "missing") writeFileSync(join(root, "scripts/ui/frozen-retirements.json"), JSON.stringify([{
      source: "src/specialized.tsx", originalCommit: retirement === "evidence" ? "0".repeat(40) : originalCommit,
      reviewReason: "Explicit route extraction; browser acceptance pending", acceptance: "pending",
      successors: retirement === "empty" ? [] : [{ source: "src/successor.tsx", fingerprint: fingerprint("reviewed successor") }],
    }]));
    if (retirement === "reappeared") writeFileSync(join(root, "src/specialized.tsx"), "retained layout");
  }
  const result = spawnSync(process.execPath, [join(root, "scripts/ui/check-standard.mjs"), ...(complete ? ["--complete"] : [])], { encoding: "utf8" });
  if (result.error) throw result.error;
  return { status: result.status, output: result.stdout + result.stderr };
}

afterEach(() => {
  for (const root of roots.splice(0)) rmSync(root, { recursive: true, force: true });
});

describe("shared UI migration guard", () => {
  it("accepts an explicit aliased control and an unchanged specialized source", () => {
    const result = fixture();
    expect(result.status).toBe(0);
    expect(result.output).toContain("Migrated caller presentation guard passed.");
  });

  it("rejects missing appearance on an aliased imported control", () => {
    const result = fixture({ content: 'import { Button as Action } from "@/components/ui/button"; export const View = () => <Action>Open</Action>;' });
    expect(result.status).toBe(1);
    expect(result.output).toContain("Action lacks explicit appearance");
  });

  it("rejects native tables and legacy presentation classes", () => {
    const result = fixture({ content: 'export const View = () => <div className="form-actions"><table /></div>;' });
    expect(result.status).toBe(1);
    expect(result.output).toContain("native table bypasses shared presentation");
    expect(result.output).toContain("legacy presentation class");
  });

  it("rejects a changed frozen source instead of refreshing its baseline", () => {
    const result = fixture({ frozenChanged: true });
    expect(result.status).toBe(1);
    expect(result.output).toContain("src/specialized.tsx: frozen boundary changed");
  });

  it("accepts reviewed retirement while retaining original Git evidence and successor hashes", () => {
    expect(fixture({ retirement: "reviewed" }).status).toBe(0);
  });

  it.each([
    ["missing", "frozen boundary missing without reviewed retirement"],
    ["changed", "retired successor missing or changed"],
    ["evidence", "original frozen evidence unavailable or changed"],
    ["empty", "invalid frozen retirement evidence"],
    ["reappeared", "retired entrypoint reappeared"],
  ])("rejects %s retirement evidence", (retirement, message) => {
    const result = fixture({ retirement });
    expect(result.status).toBe(1);
    expect(result.output).toContain(message);
  });

  it("does not turn retirement reconciliation into specialized browser acceptance", () => {
    const result = fixture({ retirement: "reviewed", complete: true });
    expect(result.status).toBe(1);
    expect(result.output).toContain("retired boundaries require specialized browser acceptance");
  });

  it("rejects an ordinary component reached across a specialized boundary", () => {
    const result = fixture({ boundary: true });
    expect(result.status).toBe(1);
    expect(result.output).toContain("src/specialized.tsx: src/view.tsx needs a legacy boundary adapter");
  });

  it("rejects stale source inventories", () => {
    const result = fixture({ stale: true });
    expect(result.status).toBe(1);
    expect(result.output).toContain("inventory is stale; regenerate coverage");
  });

  it("keeps full browser/state acceptance separate from the source guard", () => {
    const result = fixture({ complete: true });
    expect(result.status).toBe(1);
    expect(result.output).toContain("component/page sources lack current state acceptance");
    expect(result.output).toContain("routes lack current browser acceptance");
  });
});

import {vi} from "vitest";
import {deflateSync} from "node:zlib";
import {applyUiAcceptance,uiTargetRequirements,validateUiAcceptance} from "../../../scripts/ui/acceptance.mjs";
import {assertBrowserCasePassed,type BrowserCase} from "../../../scripts/testing/browser-results";
import type {UiBrowserEvidenceWitness} from "../../../scripts/testing/ui-evidence-witness";

type UiContractBinding={path:string;sha256:string;bytes:number};
type UiContractObservation={case:BrowserCase;report:UiContractBinding;observation:UiContractBinding;screenshot:UiContractBinding;state:UiContractBinding};
type UiContractTarget={kind:"route"|"source";source:string;fingerprint:string;observations:UiContractObservation[]};
type UiContractReceipt={schema:string;id:string;result:string;sourceFingerprint:string;verificationFingerprint:string;candidate:string;regression:{native:UiContractBinding;aggregate:UiContractBinding;release:UiContractBinding;releaseFiles:UiContractBinding;browsers:UiContractBinding[]};visualReview:UiContractBinding;targets:UiContractTarget[];transitions:Record<string,unknown>[]};
function uiContractPng(width:number){
 const crc=(data:Buffer)=>{let value=0xffffffff;for(const byte of data){value^=byte;for(let bit=0;bit<8;bit++)value=value&1?0xedb88320^(value>>>1):value>>>1;}return(value^0xffffffff)>>>0;};
 const chunk=(type:string,data:Buffer)=>{const name=Buffer.from(type),length=Buffer.alloc(4),checksum=Buffer.alloc(4);length.writeUInt32BE(data.length);checksum.writeUInt32BE(crc(Buffer.concat([name,data])));return Buffer.concat([length,name,data,checksum]);},header=Buffer.alloc(13);header.writeUInt32BE(width);header.writeUInt32BE(320,4);header[8]=8;header[9]=6;
 return Buffer.concat([Buffer.from([137,80,78,71,13,10,26,10]),chunk("IHDR",header),chunk("IDAT",deflateSync(Buffer.alloc((width*4+1)*320))),chunk("IEND",Buffer.alloc(0))]);
}
/** Controlled metadata ports exercise the actual validator. They do not run or attest to a build,
 * native regression, business owner or browser; production loadUiAcceptance verifies those separately. */
function uiAcceptanceContract(){
 const files=new Map<string,Buffer>(),sourceFiles=new Map<string,Buffer>(),original=Buffer.from("original specialized geometry"),current={runtimeFingerprint:"1".repeat(64),verificationFingerprint:"2".repeat(64),candidate:"a".repeat(40)},release={id:"compiled-current-candidate",fileManifestSha256:"3".repeat(64)};
 const owner="src/owner.tsx",page="src/app/example/page.tsx";sourceFiles.set(owner,Buffer.from("export const Owner=()=>null;"));sourceFiles.set(page,Buffer.from("export default function Page(){return null;}"));
 const fileHash=(path:string)=>createHash("sha256").update(sourceFiles.get(path)!).digest("hex"),closurePaths=[owner,page].sort(),closureIds=closurePaths.map((_,index)=>index),routeHash=fingerprint(closurePaths.map(path=>`${path}:${fileHash(path)}`).sort().join("\n"));
 const inventory={sources:[{source:owner,scope:"standardize",fingerprint:fileHash(owner),surfaces:[],styles:[],queryViews:[],accessChecks:[],verification:"pending"}],styleSources:[],closurePaths,routes:[{source:page,route:"/example",scope:"standardize",fingerprint:routeHash,closureIds,dependencies:[owner,page],styles:[],stateChecklist:[],queryViews:[],verification:"pending",evidence:[]}]};
 const store=(path:string,value:unknown):UiContractBinding=>{const bytes=Buffer.isBuffer(value)?value:Buffer.from(JSON.stringify(value));files.set(path,bytes);return{path,sha256:createHash("sha256").update(bytes).digest("hex"),bytes:bytes.length};};
 const read=(binding:UiContractBinding)=>{const bytes=files.get(binding.path);if(!bytes||createHash("sha256").update(bytes).digest("hex")!==binding.sha256)throw Error("UI_BOUND_EVIDENCE_CHANGED");return bytes;};
 const browserCases:(BrowserCase&{passed:boolean;run:string;report:string;cleanup:{completed:boolean;ownedDirectoryAbsent:boolean};uiEvidence?:UiBrowserEvidenceWitness})[]=[],visual={schema:"erpx-ui-visual-review/v1",result:"passed",sourceFingerprint:current.runtimeFingerprint,verificationFingerprint:current.verificationFingerprint,candidate:current.candidate,releaseId:release.id,reviewer:"Metadata contract fixture only",reviewedImages:[] as string[],findings:[]};
 const regression={native:store("docs/testing/evidence/native.json",{}),aggregate:store("docs/testing/evidence/aggregate.json",{}),release:store(".runtime/releases/fixture/release.json",{}),releaseFiles:store(".runtime/releases/fixture/release-files.json",{}),browsers:[store("docs/testing/evidence/browser.json",{})]};
 const frozen:{source:string;fingerprint:string}[]=[{source:owner,fingerprint:fingerprint(original.toString())}],retirements:Record<string,unknown>[]=[];
 const receipt:UiContractReceipt={schema:"erpx-ui-acceptance-receipt/v1",id:"contract-owner",result:"passed",sourceFingerprint:current.runtimeFingerprint,verificationFingerprint:current.verificationFingerprint,candidate:current.candidate,regression,visualReview:store("docs/testing/evidence/visual.json",visual),targets:[],transitions:[]};
 for(const kind of ["route","source"] as const){
  const target:UiContractTarget={kind,source:kind==="route"?page:owner,fingerprint:kind==="route"?routeHash:fileHash(owner),observations:[]},requirements=uiTargetRequirements(inventory,target,frozen.map(row=>row.source));
  const cells=[320,390,768,1024,1440].flatMap(width=>["light","dark","system"].map(theme=>({width,theme,resolvedTheme:theme==="system"?"light":theme})));cells.push({width:390,theme:"system",resolvedTheme:"dark"});
  for(const [index,cell]of cells.entries()){
   const run=(kind==="route"?"1":"2")+index.toString(16).padStart(11,"0"),directory=`docs/testing/evidence/${kind}-${index}`;const wanted:BrowserCase={file:"tests/e2e/shared.ui/contract.spec.ts",title:`${kind} ${index}`,project:cell.width<768?"mobile":"desktop",expectedStatus:"passed"},screenshot=store(`${directory}/view.png`,uiContractPng(cell.width));
   const observation=store(`${directory}/observation.json`,{run,schema:"erpx-ui-browser-observation/v1",result:"passed",sourceFingerprint:current.runtimeFingerprint,verificationFingerprint:current.verificationFingerprint,candidate:current.candidate,releaseId:release.id,releaseManifestSha256:release.fileManifestSha256,targetSource:target.source,targetKind:kind,targetFingerprint:target.fingerprint,routeSource:page,url:"http://127.0.0.1:3104/example",ownerContext:"fictional Company and current Work role",authorityBranch:"original owner allowed",authentication:"normal-native-owner",viewport:{width:cell.width,height:320},deviceScaleFactor:1,theme:cell.theme,resolvedTheme:cell.resolvedTheme,checks:requirements,screenshot});
   const report=store(`${directory}/result.json`,{suites:[{specs:[{file:wanted.file,title:wanted.title,tests:[{projectName:wanted.project,expectedStatus:"passed",status:"expected",results:[{status:"passed"}]}]}]}]});const state=store(`${directory}/state.json`,{run,sourceFingerprint:current.runtimeFingerprint,softwareVersion:`release-${current.runtimeFingerprint}`,release,ready:true,port:3104,origin:"http://127.0.0.1:3104",pid:101,jobsPid:102,webEntrypoint:`/fixture/.runtime/browser/${run}/release/server.js`,jobsEntrypoint:`/fixture/.runtime/browser/${run}/release/tools/jobs.mjs`});browserCases.push({...wanted,passed:true,run,report:report.path,cleanup:{completed:true,ownedDirectoryAbsent:true}});target.observations.push({case:wanted,report,observation,screenshot,state});visual.reviewedImages.push(screenshot.sha256);
  }
  receipt.targets.push(target);
 }
 visual.reviewedImages=[...new Set(visual.reviewedImages)];
 const candidate=vi.fn(async()=>({...current,passed:true,release,browserCases})),ports={bytes:read,json:(binding:UiContractBinding):unknown=>JSON.parse(read(binding).toString()),source:(path:string)=>sourceFiles.get(path)!,exists:(path:string)=>sourceFiles.has(path),git:()=>original,browserCase:assertBrowserCasePassed,caseWitness:(native:{report:string},observation:UiContractObservation)=>{if(native.report!==observation.report.path||observation.state.path!==native.report.replace(/result\.json$/,"state.json"))throw Error("UI_EXACT_CASE_RETAINED_STATE_REQUIRED");},candidate};
 let originalEvidenceFrozen=false;
 const originalCaseObservations=new Map(browserCases.map(native=>[native,receipt.targets.flatMap(target=>target.observations).find(row=>row.case.file===native.file&&row.case.title===native.title&&row.case.project===native.project)!]));
 function sealOriginalEvidence(){for(const native of browserCases){const observation=originalCaseObservations.get(native)!;native.uiEvidence=structuredClone({schema:"erpx-ui-original-browser-leaves/v1",version:1,run:native.run,report:observation.report,state:observation.state,observations:[observation.observation],screenshots:[observation.screenshot]});}}
 function freezeOriginalEvidence(){sealOriginalEvidence();originalEvidenceFrozen=true;}
 function run(){if(!originalEvidenceFrozen)sealOriginalEvidence();receipt.visualReview=store(receipt.visualReview.path,visual);const entry={id:receipt.id,...store("docs/testing/evidence/receipt.json",receipt)};return validateUiAcceptance({index:{version:1,records:[entry]},inventory,frozen,retirements,current},ports);}
 function mutate(binding:UiContractBinding,change:(value:Record<string,unknown>)=>void){const value:Record<string,unknown>=JSON.parse(read(binding).toString());change(value);Object.assign(binding,store(binding.path,value));}
 return{run,inventory,current,receipt,visual,files,frozen,retirements,ports,candidate,owner,page,original,mutate,store,freezeOriginalEvidence,browserCases};
}

describe("strict UI acceptance evidence contract (metadata fixtures, not native acceptance)",()=>{
 it("retains the no-proof default and discards forged generated accepted strings",async()=>{
  const c=uiAcceptanceContract();c.inventory.sources[0].verification="accepted";c.inventory.routes[0].verification="accepted";
  const empty=await validateUiAcceptance({index:{version:1,records:[]},inventory:c.inventory,frozen:[],retirements:[],current:c.current},c.ports);applyUiAcceptance(c.inventory,empty);expect(c.inventory.sources[0].verification).toBe("pending");expect(c.inventory.routes[0].verification).toBe("pending");expect(c.candidate).not.toHaveBeenCalled();
 });
 it("derives acceptance again from complete current bound evidence instead of carrying a previous generated flag",async()=>{
  const c=uiAcceptanceContract(),accepted=await c.run();expect(accepted.sources.has(c.owner)).toBe(true);expect(accepted.routes.has(c.page)).toBe(true);applyUiAcceptance(c.inventory,accepted);expect(c.inventory.routes[0].verification).toBe("accepted");c.inventory.routes[0].verification="pending";applyUiAcceptance(c.inventory,await c.run());expect(c.inventory.routes[0].verification).toBe("accepted");
 });
 it("rejects a passed-only forged native wrapper before any candidate proof port can accept it",async()=>{
  const c=uiAcceptanceContract();Reflect.set(c.receipt,"regression",{passed:true});await expect(c.run()).rejects.toThrow("UI_NATIVE_AND_BROWSER_REGRESSION_BINDINGS_REQUIRED");expect(c.candidate).not.toHaveBeenCalled();
 });
 it("rejects stale source, verification and candidate identities",async()=>{
  for(const field of ["sourceFingerprint","verificationFingerprint","candidate"] as const){const c=uiAcceptanceContract();c.receipt[field]="f".repeat(field==="candidate"?40:64);await expect(c.run()).rejects.toThrow("UI_CURRENT_CANDIDATE_RECEIPT_REQUIRED");}
 });
 it("requires every width/theme cell and both actual System media outcomes",async()=>{
  const missing=uiAcceptanceContract();missing.receipt.targets[0].observations=missing.receipt.targets[0].observations.filter((_,index)=>index!==0);await expect(missing.run()).rejects.toThrow("UI_REQUIRED_BROWSER_MATRIX_INCOMPLETE");
  const system=uiAcceptanceContract();system.receipt.targets[0].observations.pop();await expect(system.run()).rejects.toThrow("UI_REQUIRED_BROWSER_MATRIX_INCOMPLETE");
 });
 it("does not count skipped, expected-failure or unrelated report cases",async()=>{
  for(const bad of ["skipped","failed"]){const c=uiAcceptanceContract(),row=c.receipt.targets[0].observations[0];c.mutate(row.report,value=>{Reflect.set(value,"suites",[{specs:[{file:row.case.file,title:row.case.title,tests:[{projectName:row.case.project,expectedStatus:bad,status:"expected",results:[{status:bad}]}]}]}]);});await expect(c.run()).rejects.toThrow("REQUIRED_BROWSER_CASE_DID_NOT_PASS");}
  const absent=uiAcceptanceContract();absent.receipt.targets[0].observations[0].case={...absent.receipt.targets[0].observations[0].case,title:"not in actual regression"};await expect(absent.run()).rejects.toThrow("UI_CASE_NOT_IN_FULL_REGRESSION");
 });
 it("rejects changed report bytes, truncated images and incorrect native viewport dimensions",async()=>{
  const changed=uiAcceptanceContract();changed.files.set(changed.receipt.targets[0].observations[0].report.path,Buffer.from("changed"));await expect(changed.run()).rejects.toThrow("UI_BOUND_EVIDENCE_CHANGED");
  const image=uiAcceptanceContract(),row=image.receipt.targets[0].observations[0],truncated=image.store(row.screenshot.path,Buffer.from("not a PNG"));Object.assign(row.screenshot,truncated);image.mutate(row.observation,value=>Reflect.set(value,"screenshot",truncated));image.visual.reviewedImages.push(truncated.sha256);await expect(image.run()).rejects.toThrow("UI_NATIVE_PNG_REQUIRED");
  const size=uiAcceptanceContract();size.mutate(size.receipt.targets[0].observations[0].observation,value=>Reflect.set(value,"viewport",{width:390,height:320}));await expect(size.run()).rejects.toThrow("UI_NATIVE_VIEWPORT_IMAGE_MISMATCH");
 });
 it("does not promote partial owner state coverage or an unrelated route",async()=>{
  const partial=uiAcceptanceContract();partial.mutate(partial.receipt.targets[0].observations[0].observation,value=>Reflect.set(value,"checks",["loaded"]));await expect(partial.run()).rejects.toThrow("UI_REQUIRED_BROWSER_MATRIX_INCOMPLETE");
  const route=uiAcceptanceContract();route.mutate(route.receipt.targets[0].observations[0].observation,value=>Reflect.set(value,"url","http://127.0.0.1:3104/unrelated"));await expect(route.run()).rejects.toThrow("UI_BROWSER_OWNER_ENTRY_REQUIRED");
 });
 it("rejects a source edit even when someone preserves the generated inventory fingerprint",async()=>{
  const c=uiAcceptanceContract();c.ports.source=()=>Buffer.from("newer source bytes");await expect(c.run()).rejects.toThrow("UI_ROUTE_CLOSURE_CHANGED");
 });
 it("requires current visual review of exactly the actual image set",async()=>{
  const c=uiAcceptanceContract();c.visual.reviewedImages.push("f".repeat(64));await expect(c.run()).rejects.toThrow("UI_VISUAL_REVIEW_IMAGE_SET_MISMATCH");
 });
 it("requires original frozen Git bytes and complete current successors/callers for an explicit migration transition",async()=>{
  const c=uiAcceptanceContract(),currentBinding={path:c.owner,sha256:c.inventory.sources[0].fingerprint};c.receipt.transitions.push({source:c.owner,purpose:"authorized-product-migration",originalCommit:"b".repeat(40),originalFingerprint:c.frozen[0].fingerprint,originalSuccessors:[],current:{state:"present",binding:currentBinding},successors:[currentBinding],routeSources:[c.page]});c.files.set(c.owner,c.ports.source(c.owner));
  expect((await c.run()).transitions.has(c.owner)).toBe(true);c.ports.git=()=>Buffer.from("different original");await expect(c.run()).rejects.toThrow("UI_ORIGINAL_FROZEN_EVIDENCE_REQUIRED");
 });
 it("refuses a transition with missing current caller coverage instead of waiving the old frozen contract",async()=>{
  const c=uiAcceptanceContract(),currentBinding={path:c.owner,sha256:c.inventory.sources[0].fingerprint};c.receipt.transitions.push({source:c.owner,purpose:"authorized-product-migration",originalCommit:"b".repeat(40),originalFingerprint:c.frozen[0].fingerprint,originalSuccessors:[],current:{state:"present",binding:currentBinding},successors:[currentBinding],routeSources:[]});c.files.set(c.owner,c.ports.source(c.owner));await expect(c.run()).rejects.toThrow("UI_TRANSITION_CALLER_CLOSURE_UNACCEPTED");
 });
 it("refuses a source-matched development cohort falsely labelled as the compiled candidate",async()=>{
  const c=uiAcceptanceContract();c.mutate(c.receipt.targets[0].observations[0].state,value=>Reflect.set(value,"release",{id:"different-release",fileManifestSha256:"3".repeat(64)}));await expect(c.run()).rejects.toThrow("UI_NATIVE_COMPILED_COHORT_REQUIRED");
 });

});



describe("original native UI leaf affinity",()=>{
 it("refuses a newly hashed report reattachment after the native case witness was captured",async()=>{
  const c=uiAcceptanceContract(),observation=c.receipt.targets[0].observations[0];c.freezeOriginalEvidence();c.mutate(observation.report,value=>Reflect.set(value,"suites",[{specs:[{file:observation.case.file,title:observation.case.title,tests:[{projectName:observation.case.project,expectedStatus:"passed",status:"expected",results:[{status:"passed",attachments:[{name:"erpx-ui-observation",contentType:"application/json",path:observation.observation.path},{name:"post-run-view",contentType:"image/png",path:observation.screenshot.path}]}]}]}]}]));await expect(c.run()).rejects.toThrow("UI_ORIGINAL_CASE_LEAF_CHANGED");
 });
 it("refuses a newly hashed observation or retained state while original native case metadata stays unchanged",async()=>{
  for(const leaf of ["observation","state"] as const){const c=uiAcceptanceContract();c.freezeOriginalEvidence();c.mutate(c.receipt.targets[0].observations[0][leaf],value=>Reflect.set(value,"postRunRebinding",true));await expect(c.run()).rejects.toThrow("UI_ORIGINAL_CASE_LEAF_CHANGED");}
 });
 it("requires an original native leaf witness rather than accepting legacy path-only records",async()=>{
  const c=uiAcceptanceContract();c.freezeOriginalEvidence();delete c.browserCases[0].uiEvidence;await expect(c.run()).rejects.toThrow("UI_ORIGINAL_CASE_LEAF_WITNESS_REQUIRED");
 });
 it("refuses public-development observations paired with an owned compiled3104 case",async()=>{
  const c=uiAcceptanceContract();c.mutate(c.receipt.targets[0].observations[0].observation,value=>Reflect.set(value,"url","https://people-dev-erpx.jesadakorn.com/example"));await expect(c.run()).rejects.toThrow("UI_OWNED_COMPILED_ORIGIN_REQUIRED");
 });
});


import {assertProductHostUiMetadata,productHostUiProjection,type ProductHostUiState} from "../../../scripts/testing/product-host-ui-evidence";
import type {ProductHostLease,ProductHostPlan} from "../../../scripts/testing/product-host-cohort";
import {productIds as uiProductIds,developmentProductOrigin as uiProductOrigin} from "../../../src/lib/commercial/product-registry";
/** Controlled metadata ports only. Real loadUiAcceptance additionally reads the
 * original lease/arm, validates native TLS/key/SPKI and hashes actual tools. */
function uiLeasedProductContract(change?:(lease:ProductHostLease,plan:ProductHostPlan)=>void){
 const c=uiAcceptanceContract(),root="/fixture",leases:UiContractBinding[]=[],contexts=new Map<string,{lease:ProductHostLease;plan:ProductHostPlan}>();
 c.inventory.routes[0].route="/holdings/[holdingSlug]/finance/documents";
 for(const target of c.receipt.targets)for(const observation of target.observations){
  const native=c.browserCases.find(row=>row.title===observation.case.title&&row.project===observation.case.project)!;
  c.mutate(observation.state,value=>{value.prefix=`erpx_browser_${native.run}`;value.productHosts=true;});
  c.mutate(observation.observation,value=>{value.url="https://finance-dev-erpx.jesadakorn.com/holdings/simulation/branches/central/finance/documents?search=original&cursor=next";});
  const state=JSON.parse(c.ports.bytes(observation.state).toString()) as ProductHostUiState,run=native.run,directory=native.report.replace(/\/result\.json$/,"");
  const plan:ProductHostPlan={version:1,enabled:true,expiresAt:Date.now()+60_000,runtimeFingerprint:c.current.runtimeFingerprint,verificationFingerprint:c.current.verificationFingerprint,release:{path:`${root}/.runtime/releases/${state.release.id}/release.json`,sha256:"4".repeat(64)},releaseFiles:{path:`${root}/.runtime/releases/${state.release.id}/release-files.json`,sha256:state.release.fileManifestSha256},tls:{certificate:{path:root+"/.scratch/dev/private-cert.pem",sha256:"5".repeat(64)},key:{path:root+"/.scratch/dev/private-key.pem",sha256:"6".repeat(64)},ca:{path:root+"/.scratch/dev/private-ca.pem",sha256:"7".repeat(64)},spki:"8".repeat(64)},tools:{node:{path:root+"/.runtime/node/node-v26.11.1-darwin-arm64/bin/node",sha256:"9".repeat(64)},chromium:{path:root+"/original-chromium",sha256:"0".repeat(64)}},allowIsolatedNormalMfa:true,allowIsolatedNormalCommercial:true};
  const lease:ProductHostLease={schema:"erpx-owned-product-host-browser/v1",run,pid:103,port:3102,upstream:"http://127.0.0.1:3104",sourceFingerprint:c.current.runtimeFingerprint,verificationFingerprint:c.current.verificationFingerprint,release:state.release,plan:c.store(directory+"/product-arm.json",plan),certificateSha256:plan.tls.certificate.sha256,ca:plan.tls.ca,chromium:plan.tools.chromium,spki:plan.tls.spki,origins:([...uiProductIds,"account","apps","admin"] as const).map(uiProductOrigin),readOnlyBusiness:true,scopedBusinessWorkflow:null,isolatedNormalCommercial:true,wholeProductAcceptance:false,ui:{version:1,observedAt:new Date(Date.now()-1000).toISOString(),listener:{address:"127.0.0.1",port:3102,pid:103},node:plan.tools.node,source:{path:root+"/scripts/testing/product-host-cohort.ts",sha256:"a".repeat(64)},backend:{pid:101,jobsPid:102,webEntrypoint:state.webEntrypoint!,jobsEntrypoint:state.jobsEntrypoint!,state:{path:`${root}/.runtime/browser/${run}/state.json`,sha256:observation.state.sha256,bytes:observation.state.bytes}}}};
  if(change)change(lease,plan);lease.plan=c.store(directory+"/product-arm.json",plan);const binding=c.store(directory+"/product-host-lease.json",lease);leases.push(binding);contexts.set(binding.path,{lease,plan});Reflect.set(native,"productHostLease",binding);
 }
 c.freezeOriginalEvidence();for(const native of c.browserCases)native.uiEvidence!.productHostLease=structuredClone(Reflect.get(native,"productHostLease"));
 Reflect.set(c.ports,"productHost",async(native:typeof c.browserCases[number],state:ProductHostUiState,observation:UiContractObservation)=>{const binding=native.uiEvidence!.productHostLease!,context=contexts.get(binding.path);if(!context)throw Error("UI_ORIGINAL_PRODUCT_HOST_LEASE_REQUIRED");c.ports.bytes(binding);assertProductHostUiMetadata(context.lease,context.plan,state,{root,run:native.run,sourceFingerprint:c.current.runtimeFingerprint,verificationFingerprint:c.current.verificationFingerprint,release:state.release,stateBinding:observation.state,chromiumPath:root+"/original-chromium"});return{projectUrl:productHostUiProjection};});
 return{...c,leases};
}

describe("lease-bound product-host UI ingestion (metadata contracts, never browser acceptance)",()=>{
 it("does not promote a read-only UI observation from a workflow-write lease",async()=>{const c=uiLeasedProductContract(lease=>{lease.scopedBusinessWorkflow="work.request-draft";});await expect(c.run()).rejects.toThrow("UI_PRODUCT_HOST_LEASE_SOURCE_REQUIRED");});
 it("accepts complete canonical Finance observations projected through the original scoped route with every matrix cell unchanged",async()=>{const c=uiLeasedProductContract(),accepted=await c.run();expect(accepted.routes.has(c.page)).toBe(true);expect(accepted.sources.has(c.owner)).toBe(true);expect(c.receipt.targets.every(target=>target.observations.length===16)).toBe(true);});
 it("rejects canonical development or forged HTTPS without the original case lease, and refuses rebinding the witnessed lease",async()=>{const absent=uiLeasedProductContract();delete absent.browserCases[0].uiEvidence!.productHostLease;await expect(absent.run()).rejects.toThrow("UI_ORIGINAL_PRODUCT_HOST_LEASE_REQUIRED");const forged=uiLeasedProductContract();Reflect.set(forged.browserCases[0],"productHostLease",{...forged.leases[0],sha256:"f".repeat(64)});await expect(forged.run()).rejects.toThrow("UI_ORIGINAL_PRODUCT_HOST_LEASE_CHANGED");});
 it("refuses a different run, source, verification, release or arbitrary origin even if all metadata was initially bound",async()=>{for(const field of ["run","sourceFingerprint","verificationFingerprint"] as const){const c=uiLeasedProductContract(lease=>{lease[field]="f".repeat(field==="run"?12:64);});await expect(c.run()).rejects.toThrow("UI_PRODUCT_HOST_LEASE_SOURCE_REQUIRED");}const release=uiLeasedProductContract(lease=>{lease.release={...lease.release,id:"another-release"};});await expect(release.run()).rejects.toThrow("UI_PRODUCT_HOST_LEASE_SOURCE_REQUIRED");for(const origin of ["https://unowned.invalid","https://sales-dev-erpx.jesadakorn.com","http://127.0.0.1:3104"]){const c=uiLeasedProductContract();c.mutate(c.receipt.targets[0].observations[0].observation,value=>{value.url=origin+"/holdings/simulation/finance/documents";});c.freezeOriginalEvidence();for(const native of c.browserCases)native.uiEvidence!.productHostLease=structuredClone(Reflect.get(native,"productHostLease"));await expect(c.run()).rejects.toThrow("UI_PRODUCT_HOST_ROUTE_REQUIRED");}});
 it("requires exact original backend/state, source-bound arm and TLS/SPKI/tool metadata",async()=>{for(const [change,code]of [[(lease:ProductHostLease)=>{lease.ui!.backend.pid=999;},"UI_PRODUCT_HOST_OWNED_PROCESSES_REQUIRED"],[(lease:ProductHostLease)=>{lease.ui!.backend.state.sha256="f".repeat(64);},"UI_PRODUCT_HOST_ORIGINAL_STATE_REQUIRED"],[(lease:ProductHostLease)=>{lease.spki="f".repeat(64);},"UI_PRODUCT_HOST_TLS_AND_TOOLS_REQUIRED"],[(lease:ProductHostLease,plan:ProductHostPlan)=>{plan.runtimeFingerprint="f".repeat(64);},"UI_PRODUCT_HOST_ORIGINAL_ARM_REQUIRED"]] as const){const c=uiLeasedProductContract(change);await expect(c.run()).rejects.toThrow(code);}});
 it("uses actual registry/policy and native Proxy projections for18roots, Front detail and scoped query without cross-product aliases",()=>{for(const key of [...uiProductIds,"account","apps","admin"] as const)expect(productHostUiProjection(uiProductOrigin(key)+"/").pathname).toBe(key==="pos"||key==="pms"?"/front-access":"/product-entry");const front=productHostUiProjection(uiProductOrigin("pms")+"/riverside-lantern/hotel/holding/guest-accounts/records/pms_guest_account/original/overview?property=actual&cursor=next&returnTo=%2Foriginal");expect(front.pathname).toBe("/front/riverside-lantern/hotel/pms/holding/guest-accounts/records/pms_guest_account/original/overview");expect(front.search).toBe("?property=actual&cursor=next&returnTo=%2Foriginal");expect(productHostUiProjection(uiProductOrigin("finance")+"/holdings/simulation/branches/main/departments/accounts/finance/documents?search=original").pathname).toBe("/holdings/simulation/finance/documents");expect(()=>productHostUiProjection(uiProductOrigin("sales")+"/holdings/simulation/finance/documents")).toThrow("UI_PRODUCT_HOST_ROUTE_REQUIRED");expect(()=>productHostUiProjection(uiProductOrigin("finance")+"/api/v1/holdings/simulation/finance/invoices")).toThrow("UI_PRODUCT_HOST_RENDERED_ROUTE_REQUIRED");});

});


it("preserves both native Front handoff routes and their original query and hash on POS and PMS",()=>{
 for(const product of ["pos","pms"] as const)for(const pathname of ["/launch","/choose-work-role"]){
  const observed=uiProductOrigin(product)+pathname+"?company=original-company&revision=7&returnTo=%2Fholdings%2Foriginal%2F"+product+"%3Fcursor%3Dnext#original-owner",projected=productHostUiProjection(observed);
  expect(projected.href).toBe(observed);
  expect(projected.pathname).toBe(pathname);
  expect(projected.search).toBe(new URL(observed).search);
  expect(projected.hash).toBe("#original-owner");
 }
});
