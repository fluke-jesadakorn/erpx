import { describe, expect, test } from "vitest";
import { assertBrandCategoryLocks, assertBrandIndependentPublisher } from "@erpx/server-brand/domain/policy";
import { brandIdentitySchema, type ActiveBrandIdentity, type BrandCategoryLock, type BrandDraft } from "@erpx/server-brand/domain/identity";
import { readBrandIdentityWorkspace } from "@erpx/server-brand/application/identity-read";
import * as brandPublic from "@erpx/server-brand/public/brand-identity";
import * as brandIndex from "@erpx/server-brand/index";

const identity = { identity: { shortName: "ERPX", tagline: "Approved identity" }, visual: { accentColor: "#224488", logoPublicationId: null } };
const reason = "Independently review the identity";
const draft: BrandDraft = { id: "draft-1", company_id: "company-1", based_revision: 0, identity_json: JSON.stringify(identity), status: "draft", version: 1, created_by: "maker", updated_by: "editor", updated_at: 10 };

describe("Retained Brand identity contract", () => {
  test("trims bounded identity values and normalizes the accent color", () => {
    const parsed = brandIdentitySchema.parse({ identity: { shortName: "  ERPX  ", tagline: "  Approved identity  " }, visual: { accentColor: "#AABBCC", logoPublicationId: null } });
    expect(parsed).toEqual({ identity: { shortName: "ERPX", tagline: "Approved identity" }, visual: { accentColor: "#aabbcc", logoPublicationId: null } });
    expect(brandIdentitySchema.parse({ identity: { shortName: "ERPX" }, visual: { accentColor: "#224488", logoPublicationId: "publication" } })).toMatchObject({ identity: { tagline: "" } });
  });
  test("rejects unknown fields and out-of-bounds identity values", () => {
    for (const invalid of [
      { ...identity, implementation: "hidden" },
      { identity: { shortName: "", tagline: "Approved" }, visual: identity.visual },
      { identity: { shortName: "x".repeat(41), tagline: "Approved" }, visual: identity.visual },
      { identity: { shortName: "ERPX", tagline: "x".repeat(161) }, visual: identity.visual },
      { identity: identity.identity, visual: { accentColor: "#22448", logoPublicationId: null } },
      { identity: identity.identity, visual: { accentColor: "#224488", logoPublicationId: "" } },
      { identity: identity.identity, visual: { accentColor: "#224488" } },
    ]) expect(brandIdentitySchema.safeParse(invalid).success).toBe(false);
  });
  test("compares approved category locks against canonical sections", () => {
    const current = brandIdentitySchema.parse(identity);
    const locks: BrandCategoryLock[] = (["identity", "visual"] as const).map(category => ({ category, locked: true, approved_snapshot_json: JSON.stringify(current[category]), version: 1, reason, updated_by: "owner", updated_at: 10 }));
    expect(() => assertBrandCategoryLocks(locks, current)).not.toThrow();
    expect(() => assertBrandCategoryLocks(locks, { ...current, visual: { ...current.visual, accentColor: "#778899" } })).toThrow(/visual/);
    expect(() => assertBrandCategoryLocks(locks, { ...current, identity: { ...current.identity, shortName: "Another name" } })).toThrow(/identity/);
    expect(() => assertBrandCategoryLocks([{ ...locks[0], locked: false }], { ...current, identity: { ...current.identity, shortName: "Another name" } })).not.toThrow();
  });
  test("requires another person for independent publication", () => {
    expect(() => assertBrandIndependentPublisher(id => id === "reviewer", "maker", "editor")).not.toThrow();
    for (const isSamePerson of [() => true, ((id:string) => id === "maker"), ((id:string) => id === "editor")]) {
      expect(() => assertBrandIndependentPublisher(isSamePerson, "maker", "editor")).toThrow(/another authorized person/i);
    }
  });
  test("parses a retained published identity and tolerates an unavailable lifecycle table", async () => {
    const row = { ...draft, revision: 1, draft_id: draft.id, draft_version: 2, published_by: "reviewer", published_at: 20, active_version: 3 } as unknown as ActiveBrandIdentity;
    expect(await readBrandIdentityPorts(true, row)).toMatchObject({ active_version: 3, identity });
    expect(await readBrandIdentityPorts(false, row)).toBeNull();
  });
  test("workspace view hides history and other drafts using provided authority facts", async () => {
    const output = await readBrandIdentityWorkspace({ actorId: "worker", canManage: false, canLock: false, async authorize() {}, async drafts(includeOthers) { expect(includeOthers).toBe(false); return []; }, async versions() { return []; }, async locks() { return []; }, async history() { return [{ id: "private" }] as never; }, async active() { return null; }, async legacy() { return { shortName: "Legacy", accentColor: "#123456" }; }, async media() { return []; } });
    expect(output).toMatchObject({ history: [], initial: { identity: { shortName: "Legacy", tagline: "" }, visual: { logoPublicationId: null } }, canManage: false });
  });
  test("retires every TypeScript identity writer from the public entrypoints", () => {
    for (const entrypoint of [brandPublic, brandIndex]) for (const writer of ["saveBrandIdentityDraft", "publishBrandIdentity", "rollbackBrandIdentity", "changeBrandCategoryLock", "cancelBrandDraft"])
      expect(Object.hasOwn(entrypoint, writer)).toBe(false);
    expect(typeof brandPublic.publishedBrandIdentity).toBe("function");
    expect(typeof brandPublic.brandIdentityWorkspace).toBe("function");
  });
});

async function readBrandIdentityPorts(available: boolean, row: ActiveBrandIdentity | null) {
  const { readPublishedBrandIdentity } = await import("@erpx/server-brand/application/identity-read");
  return readPublishedBrandIdentity({ async available() { return available; }, async active() { return row; } });
}
import { createBrandingApplication, normalizeBrandMedia, type BrandingPorts, type BrandMutation as BrandingMutation } from "@erpx/server-brand/application/branding";
import { storefrontPresentation, type BrandProfileRow } from "@erpx/server-brand/domain/branding";

function brandingFixture(overrides: Partial<BrandingPorts<Uint8Array>> = {}) {
 const calls:string[]=[],mutations:BrandingMutation[]=[];
 const ports:BrandingPorts<Uint8Array>={
  actor:{holdingId:"holding",holdingName:"Holding Name",holdingSlug:"holding-slug",companyId:"company",branchId:"branch"},clock:{now:()=>42},legacyAttachmentsReadable:false,
  urls:{attachment:id=>id?`attachment/${id}`:null,publication:id=>id?`publication/${id}`:null},
  async holdingAuthority(){return true;},async organizationAuthority(){return true;},async evidenceAuthority(){return true;},assertMediaWritable(){calls.push("writable");},
  async company(){return {display_name:"Company"};},async branch(id){return {id,display_name:"North Branch"};},async companyExists(){return true;},
  async profile(){return {primary_attachment_id:"old-media"} as BrandProfileRow;},async branches(){return [];},async profiles(){return [];},
  async storefront(){return {slug:"company-slug",display_name:"Company",short_name:"Company",accent_color:"#3659c9",logo_attachment_id:"old-media",logo_publication_id:null};},
  async storefrontLogo(){return {logo_attachment_id:"old-media"};},async published(){return null;},async recordBranch(){return "branch";},async evidence(){return null;},
  image:{async metadata(){return {width:64,height:64};},async encode(){return new Uint8Array([1,2,3]);}},async checksum(){return "checksum";},
  async attachmentObjectKey(){return "old-key";},
  files:{async put(key){calls.push(`put:${key}`);},async delete(key){calls.push(`delete:${key}`);}},
  async commit(mutation,result){mutations.push(mutation);calls.push("commit");return result;},...overrides,
 };
 return {app:createBrandingApplication(ports),ports,calls,mutations};
}
describe("Brand media and projection application",()=>{
 test("validates metadata through the provider port and retains provider encoding failures",async()=>{
  const invalid=brandingFixture({image:{async metadata(){throw new Error("bad decoder");},async encode(){return new Uint8Array();}}});
  await expect(invalid.app.normalize(new ArrayBuffer(0))).rejects.toMatchObject({code:"LOGO_INVALID"});
  await expect(normalizeBrandMedia({async metadata(){return {width:31,height:32};},async encode(){throw new Error("must not encode");}},new ArrayBuffer(0))).rejects.toMatchObject({code:"LOGO_DIMENSIONS"});
  const encode=brandingFixture({image:{async metadata(){return {width:64,height:64};},async encode(){throw new Error("encoder failure");}}});
  await expect(encode.app.normalize(new ArrayBuffer(0))).rejects.toThrow("encoder failure");
 });
 test("commits file replacement facts then cleans the previous object",async()=>{
  const f=brandingFixture();
  await f.app.completeUpload({id:"intent",owner_type:"branch_brand",owner_id:"branch",purpose:"primary",revision:1,original_name:"logo.png"},new ArrayBuffer(0));
  expect(f.mutations[0]).toMatchObject({action:"upload-profile",previousId:"old-media",attachmentId:"attachment-intent",bytes:3,now:42});
  expect(f.calls).toEqual(["writable","put:holding/branding/branch/branch/primary-intent.webp","commit","delete:old-key"]);
 });
 test("compensates a failed atomic write without deleting the previous object",async()=>{
  const f=brandingFixture({async commit(){throw new Error("authority revoked");}});
  await expect(f.app.completeUpload({id:"intent",owner_type:"company_storefront_brand",owner_id:"company",purpose:"logo",revision:1,original_name:"logo.png"},new ArrayBuffer(0))).rejects.toThrow("authority revoked");
  expect(f.calls).toEqual(["writable","put:holding/branding/storefront/company/logo-intent.webp","delete:holding/branding/storefront/company/logo-intent.webp"]);
 });
 test("checks media rollback before decoding and published identity before legacy edits",async()=>{
  const readonly=brandingFixture({assertMediaWritable(){throw new Error("read only");},image:{async metadata(){throw new Error("must not decode");},async encode(){return new Uint8Array();}}});
  await expect(readonly.app.completeUpload({id:"intent",owner_type:"branch_brand",owner_id:"branch",purpose:"primary",revision:1,original_name:"logo.png"},new ArrayBuffer(0))).rejects.toThrow("read only");
  const active={identity,revision:4} as ActiveBrandIdentity,published=brandingFixture({async published(){return active;},assertMediaWritable(){throw new Error("must not check rollback");}});
  await expect(published.app.updateStorefront({removeLogo:true,commandId:"key"})).rejects.toMatchObject({code:"BRAND_PUBLISHED_IDENTITY_CONTROLLED"});
 });
 test("published absence of a logo suppresses every legacy fallback",()=>{
  const f=brandingFixture(),current={slug:"company",display_name:"Company",short_name:"Old",accent_color:"#112233",logo_attachment_id:"legacy",logo_publication_id:"legacy-publication"};
  const brand=storefrontPresentation(f.ports.actor,current,{identity,revision:4} as ActiveBrandIdentity,f.ports.urls,false);
  expect(brand).toMatchObject({shortName:"ERPX",accentColor:"#224488",logoUrl:null,identityRevision:4});
 });
 test("document mode honors active branch defaults and a company-wide scope",async()=>{
  const f=brandingFixture({async profile(){return {document_mode:"both"} as BrandProfileRow;}});
  expect(await f.app.document({branchId:"branch"})).toMatchObject({mode:"both",branchName:"North Branch"});
  expect(await f.app.document({branchId:null,requestedMode:"branch"})).toMatchObject({mode:"holding",branch:null});
 });
 test("workspace view hides history and other's drafts using provided authority facts",async()=>{
  const output=await readBrandIdentityWorkspace({actorId:"worker",canManage:false,canLock:false,async authorize(){},async drafts(includeOthers){expect(includeOthers).toBe(false);return [];},async versions(){return [];},async locks(){return [];},async history(){return [{id:"private"}] as never;},async active(){return null;},async legacy(){return {shortName:"Legacy",accentColor:"#123456"};},async media(){return [];}});
  expect(output).toMatchObject({history:[],initial:{identity:{shortName:"Legacy",tagline:""},visual:{logoPublicationId:null}},canManage:false});
 });
});
