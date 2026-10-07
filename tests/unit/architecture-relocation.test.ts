import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import {afterEach,expect,test} from "vitest";
import {applyRelocation,relocationPlan} from "../../scripts/architecture/relocate-legacy";

const roots:string[]=[];
function fixture(){const root=fs.mkdtempSync(path.join(os.tmpdir(),"erpx-owner-relocation-"));roots.push(root);fs.mkdirSync(path.join(root,"src/server"),{recursive:true});return root;}
function write(root:string,file:string,text:string){fs.writeFileSync(path.join(root,file),text);}
afterEach(()=>{for(const root of roots.splice(0))fs.rmSync(root,{recursive:true,force:true});});

test("retains exact named value/type exports and resolves dependencies against the old location",()=>{
 const root=fixture();
 write(root,"src/server/branding.ts",'export class Identity {name="sample";} export type Facts={name:string};');
 write(root,"src/server/brand-identity.ts",'import {Identity} from "./branding"; export {Identity as BrandIdentity} from "./branding"; export type {Facts} from "./branding"; export function create(){return new Identity();} export type Deferred=import("./branding").Facts; export async function load(){return import("./branding");}');
 const plan=relocationPlan(root),identity=plan.find(entry=>entry.source==="src/server/brand-identity.ts")!;
 expect(identity.hazards).toEqual([]);expect(identity.exports).toEqual({values:["BrandIdentity","create","load"],types:["Deferred","Facts"]});
 expect(identity.implementation).not.toContain('"./branding"');expect(identity.implementation.match(/@\/server\/branding/g)).toHaveLength(5);
 applyRelocation(plan,root);
 expect(fs.readFileSync(path.join(root,identity.source),"utf8")).toContain('export {BrandIdentity,create,load} from "@/server/modules/brand/adapters/legacy/brand-identity"');
 expect(fs.readFileSync(path.join(root,identity.target),"utf8")).toBe(identity.implementation);
 expect(relocationPlan(root)).toEqual([]);
});

test("rejects path-sensitive code before any source is moved",()=>{
 const root=fixture(),source='export const source=new URL("./manifest.json",import.meta.url);';write(root,"src/server/brand-identity.ts",source);
 const plan=relocationPlan(root);expect(plan[0].hazards).toContain("import.meta depends on original module location");
 expect(()=>applyRelocation(plan,root)).toThrow("review of every reported hazard");expect(fs.readFileSync(path.join(root,plan[0].source),"utf8")).toBe(source);expect(fs.existsSync(path.join(root,plan[0].target))).toBe(false);
});
