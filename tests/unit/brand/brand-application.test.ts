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
import { normalizeBrandMedia } from "@erpx/server-brand/application/branding";
describe("Retained Branding media normalization", () => {
  test("validates image metadata before encoding", async () => {
    const encoded = await normalizeBrandMedia({
      async metadata() { return { width: 64, height: 64 }; },
      async encode() { return new Uint8Array([1, 2, 3]); },
    }, new ArrayBuffer(0));
    expect(Array.from(encoded)).toEqual([1, 2, 3]);
  });
  test("maps decoder and dimension failures to retained logo codes", async () => {
    await expect(normalizeBrandMedia({
      async metadata() { throw new Error("bad decoder"); },
      async encode() { return new Uint8Array(); },
    }, new ArrayBuffer(0))).rejects.toMatchObject({ code: "LOGO_INVALID" });
    await expect(normalizeBrandMedia({
      async metadata() { return { width: 31, height: 32 }; },
      async encode() { throw new Error("must not encode"); },
    }, new ArrayBuffer(0))).rejects.toMatchObject({ code: "LOGO_DIMENSIONS" });
    await expect(normalizeBrandMedia({
      async metadata() { return { width: 4097, height: 4097 }; },
      async encode() { throw new Error("must not encode"); },
    }, new ArrayBuffer(0))).rejects.toMatchObject({ code: "LOGO_PIXELS" });
  });
  test("retains encoder failures without masking them", async () => {
    await expect(normalizeBrandMedia({
      async metadata() { return { width: 64, height: 64 }; },
      async encode() { throw new Error("encoder failure"); },
    }, new ArrayBuffer(0))).rejects.toThrow("encoder failure");
  });
});
