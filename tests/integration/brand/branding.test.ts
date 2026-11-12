/**
 * Retained Branding reads, the native presentation consumer client and pure
 * media normalization.
 *
 * Branding profile/storefront/media mutations are owned by the native Rust
 * Branding command/HTTP path. The workspace/document brand, profile-list and
 * executive evidence reads consumed by serving callers are the native owner
 * projections reached through `createNativeBrandPresentationClient`; this suite
 * proves their 1:1 shape mapping, selected-work forwarding and fail-closed
 * transport. Owner-local SQL/authority assertions live in
 * `tests/integration/brand/rust-branding-postgres.test.rs` and the HTTP router
 * boundary in `tests/integration/brand/rust-branding-http.test.rs`.
 */
import sharp from "sharp";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import {
  createNativeBrandPresentationClient,
  listBrandProfiles,
  nativeBrandPresentationConfigured,
  normalizeBrandImage,
  readStorefrontAppBrand,
  recordBranchId,
  requireBrandManagement,
} from "@erpx/server-brand/public/branding";
import type { NativeBrandPresentationTransport } from "@erpx/server-brand/public/branding";
import * as brandingPublic from "@erpx/server-brand/public/branding";
import { matchesUploadSignature } from "@erpx/server-files/public/upload-validation";
import { applyMigrations, env } from "../../support/runtime-env";
import { expenseDb as db, expenseFixture } from "../../support/helpers/expense-fixture";

let fixture: Awaited<ReturnType<typeof expenseFixture>>;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
});
beforeEach(async () => {
  fixture = await expenseFixture();
});

const config = JSON.stringify({
  privateBaseUrl: "http://127.0.0.1:3199",
  timeoutMs: 5_000,
  holdings: [
    { origin: "https://erpx-internal.jesadakorn.com", holdingSlug: "erpx", holdingId: "h003" },
  ],
});

const actor = {
  userId: "user-1",
  name: "User",
  email: "user@example.test",
  assignmentId: "assignment-1",
  role: "document-role",
  companyId: "erpx-co",
  branchId: "branch-1",
  departmentId: null,
  holdingId: "h003",
  holdingSlug: "erpx",
  holdingName: "ERPX Holding",
  scope: { kind: "branch", holdingId: "h003", holdingSlug: "erpx", holdingName: "ERPX Holding", companyId: "erpx-co", branchId: "branch-1", branchSlug: "north", branchName: "North Branch" },
  assignmentScope: "branch",
  inherited: false,
  modules: ["brand"],
  assignments: [],
  workspaces: [],
  authority: { sessionId: "session-1", revision: 4, assignmentVersion: 2 },
} as unknown as WorkContext;

const incoming = { get: (name: string) => (name === "cookie" ? "erpx_session=abc" : name === "host" ? "erpx-internal.jesadakorn.com" : null) };

const workspaceRow = {
  scopeKind: "branch",
  scopeId: "branch-1",
  label: "North Branch",
  initials: "NB",
  primaryUrl: "/api/public/holdings/erpx/media-publications/pub-1/content",
  inverseUrl: "/api/v1/holdings/erpx/branches/north/attachments/att-2/content",
  documentMode: "both",
};

const storefrontRow = {
  companyId: "erpx-co",
  holdingSlug: "erpx",
  companySlug: "erpx-co",
  companyName: "ERPX Company",
  shortName: "ERPX",
  accentColor: "#224488",
  logoUrl: "/api/public/holdings/erpx/media-publications/pub-2/content",
  identityRevision: 7,
};

function stub(respond: (url: URL, headers: Headers) => unknown, calls: { url: URL; headers: Headers }[] = []) {
  const transport: NativeBrandPresentationTransport = async (url, headers) => {
    calls.push({ url, headers });
    const data = respond(url, headers);
    return new Response(JSON.stringify({ data }), {
      status: 200,
      headers: { "content-type": "application/json; charset=utf-8" },
    });
  };
  return { client: createNativeBrandPresentationClient(config, transport), calls };
}

function failing(status: number, code: string) {
  const transport: NativeBrandPresentationTransport = async () =>
    new Response(JSON.stringify({ error: { code, message: "Native refusal." } }), {
      status,
      headers: { "content-type": "application/json" },
    });
  return createNativeBrandPresentationClient(config, transport);
}

it("accepts real PNG/WebP signatures and rejects a MIME mismatch", async () => {
  const png = await sharp({ create: { width: 64, height: 64, channels: 4, background: "transparent" } }).png().toBuffer();
  const webp = await sharp(png).webp().toBuffer();
  expect(matchesUploadSignature("image/png", png)).toBe(true);
  expect(matchesUploadSignature("image/webp", webp)).toBe(true);
  expect(matchesUploadSignature("image/webp", png)).toBe(false);
  expect(matchesUploadSignature("image/png", webp)).toBe(false);
});

it("re-encodes transparent logos as metadata-free WebP", async () => {
  const source = await sharp({ create: { width: 96, height: 48, channels: 4, background: { r: 47, g: 105, b: 245, alpha: 0.4 } } })
    .withExif({ IFD0: { ImageDescription: "must not survive" } }).withMetadata()
    .png()
    .toBuffer();
  const output = await normalizeBrandImage(source.buffer.slice(source.byteOffset, source.byteOffset + source.byteLength) as ArrayBuffer);
  const metadata = await sharp(output).metadata();
  expect(metadata.format).toBe("webp");
  expect(metadata.width).toBe(96);
  expect(metadata.height).toBe(48);
  expect(metadata.hasAlpha).toBe(true);
  expect(metadata.exif).toBeUndefined();
  expect(metadata.icc).toBeUndefined();
});

it("rejects corrupt, undersized and pixel-bomb logo inputs", async () => {
  await expect(normalizeBrandImage(new Uint8Array([1, 2, 3, 4]).buffer)).rejects.toMatchObject({ code: "LOGO_INVALID" });
  const small = await sharp({ create: { width: 31, height: 32, channels: 4, background: "white" } }).png().toBuffer();
  await expect(normalizeBrandImage(small.buffer.slice(small.byteOffset, small.byteOffset + small.byteLength) as ArrayBuffer)).rejects.toMatchObject({ code: "LOGO_DIMENSIONS" });
  const huge = await sharp({ create: { width: 4097, height: 4097, channels: 3, background: "white" } }).png().toBuffer();
  await expect(normalizeBrandImage(huge.buffer.slice(huge.byteOffset, huge.byteOffset + huge.byteLength) as ArrayBuffer)).rejects.toMatchObject({ code: "LOGO_PIXELS" });
});

it("derives an expense Branch from the record and keeps the retained document-mode reader", async () => {
  const expenseId = `brand-expense-${crypto.randomUUID()}`;
  await db.prepare("INSERT INTO finance_expenses(id,company_id,branch_id,status,created_by,created_at,updated_at) VALUES(?,?,?,'draft',?,?,?)")
    .bind(expenseId, fixture.company, fixture.branch, fixture.claimant.userId, Date.now(), Date.now()).run();
  expect(await recordBranchId(db, fixture.claimant, "expense", expenseId)).toBe(fixture.branch);
});

it("allows the Holding owner to manage brands and rejects ordinary staff", async () => {
  await expect(requireBrandManagement(db, fixture.owner, "holding_brand", fixture.owner.holdingId)).resolves.toBeUndefined();
  await expect(requireBrandManagement(db, fixture.owner, "branch_brand", fixture.branch)).resolves.toBeUndefined();
  await expect(requireBrandManagement(db, fixture.owner, "company_storefront_brand", fixture.company)).resolves.toBeUndefined();
  await expect(requireBrandManagement(db, fixture.claimant, "holding_brand", fixture.owner.holdingId)).rejects.toMatchObject({ code: "BRAND_MANAGEMENT_FORBIDDEN" });
  await expect(requireBrandManagement(db, fixture.claimant, "branch_brand", fixture.branch)).rejects.toMatchObject({ code: "BRAND_MANAGEMENT_FORBIDDEN" });
});

it("retires every TypeScript Branding writer from the public entrypoint", () => {
  for (const writer of ["completeBrandUpload", "completeStorefrontBrandUpload", "updateStorefrontAppBrand", "updateBrandProfile"])
    expect(Object.hasOwn(brandingPublic, writer)).toBe(false);
  for (const read of ["resolveWorkspaceBrand", "resolveDocumentBrand", "listBrandProfiles", "readBrandProfileEvidence", "readStorefrontAppBrand", "nativeListBrandProfiles"])
    expect(typeof (brandingPublic as unknown as Record<string, unknown>)[read]).toBe("function");
  expect(typeof brandingPublic.normalizeBrandImage).toBe("function");
});

it("reads the Branding-owned storefront through the published identity overlay", async () => {
  const now = Date.now();
  await db.batch([
    db.prepare("INSERT INTO storefront_app_brands(company_id,short_name,accent_color,updated_by,updated_at) VALUES(?,'Legacy storefront','#123456',?,?)")
      .bind(fixture.company, fixture.owner.userId, now),
    db.prepare("INSERT INTO brand_identity_drafts(id,company_id,based_revision,identity_json,status,version,created_by,updated_by,created_at,updated_at) VALUES(?,?,0,?,'published',2,?,?,?,?)")
      .bind("branding-read-draft", fixture.company, JSON.stringify({ identity: { shortName: "Published storefront", tagline: "" }, visual: { accentColor: "#224488", logoPublicationId: null } }), fixture.owner.userId, fixture.head.userId, now, now),
    db.prepare("INSERT INTO brand_identity_versions(id,company_id,revision,identity_json,draft_id,draft_version,created_by,published_by,published_at) VALUES(?,?,1,?,?,1,?,?,?)")
      .bind("branding-read-version", fixture.company, JSON.stringify({ identity: { shortName: "Published storefront", tagline: "" }, visual: { accentColor: "#224488", logoPublicationId: null } }), "branding-read-draft", fixture.owner.userId, fixture.head.userId, now),
    db.prepare("INSERT INTO brand_identity_active(company_id,identity_version_id,version,updated_by,updated_at) VALUES(?,?,1,?,?)")
      .bind(fixture.company, "branding-read-version", fixture.head.userId, now),
    db.prepare("INSERT INTO organization_brand_profiles(scope_kind,scope_id,company_id,document_mode,updated_by,updated_at) VALUES('branch',?,?,'both',?,?)")
      .bind(fixture.branch, fixture.company, fixture.owner.userId, now),
  ]);
  const storefront = await readStorefrontAppBrand(db, fixture.owner);
  expect(storefront).toMatchObject({ shortName: "Published storefront", accentColor: "#224488", identityRevision: 1, logoUrl: null });
  const profiles = await listBrandProfiles(db, fixture.owner);
  expect(profiles.holding.scopeKind).toBe("holding");
  expect(profiles.branches.find(row => row.scopeId === fixture.branch)?.documentMode).toBe("both");
  expect(profiles.storefront.shortName).toBe("Published storefront");
});

describe("native Brand presentation reads", () => {
  it("maps the native workspace brand 1:1 and forwards the selected work context", async () => {
    const { client, calls } = stub(() => workspaceRow);
    expect(await client.workspace(actor, incoming)).toEqual(workspaceRow);
    expect(calls).toHaveLength(1);
    const [{ url, headers }] = calls;
    expect(url.pathname).toBe("/api/v1/holdings/erpx/branding/presentation");
    expect(url.searchParams.get("kind")).toBe("workspace");
    expect(url.searchParams.has("branch")).toBe(false);
    expect(headers.get("host")).toBe("erpx-internal.jesadakorn.com");
    expect(headers.get("cookie")).toBe("erpx_session=abc");
    expect(headers.get("x-erpx-context-company")).toBe("erpx-co");
    expect(headers.get("x-erpx-context-assignment")).toBe("assignment-1");
    expect(headers.get("x-erpx-context-revision")).toBe("4");
    expect(headers.get("x-erpx-context-assignment-version")).toBe("2");
    expect(headers.get("x-erpx-workspace-branch")).toBe("north");
    expect(headers.get("x-erpx-holding-name")).toBe("ERPX Holding");
  });

  it("requests the document Branch and mode and keeps the Holding default", async () => {
    const document = { mode: "both", companyName: "ERPX Company", branchName: "North Branch", holding: { ...workspaceRow, scopeKind: "holding", scopeId: "h003", documentMode: "holding" }, branch: workspaceRow };
    const { client, calls } = stub(() => document);
    expect(await client.document(actor, incoming, { branchId: "branch-1", requestedMode: "both" })).toEqual(document);
    expect(calls[0].url.searchParams.get("kind")).toBe("document");
    expect(calls[0].url.searchParams.get("branch")).toBe("branch-1");
    expect(calls[0].url.searchParams.get("mode")).toBe("both");
    const companyWide = { mode: "holding", companyName: "ERPX Company", branchName: null, holding: document.holding, branch: null };
    const holdingOnly = stub(() => companyWide);
    expect(await holdingOnly.client.document(actor, incoming, { branchId: null, requestedMode: "branch" })).toEqual(companyWide);
    expect(holdingOnly.calls[0].url.searchParams.has("branch")).toBe(false);
    expect(holdingOnly.calls[0].url.searchParams.has("mode")).toBe(false);
  });

  it("maps the native profile list with the storefront identity overlay", async () => {
    const { client, calls } = stub(() => ({ holding: { ...workspaceRow, scopeKind: "holding", scopeId: "h003", label: "ERPX Holding", initials: "EH", documentMode: "holding" }, branches: [workspaceRow], storefront: storefrontRow }));
    expect(await client.profiles(actor, incoming)).toEqual({
      holding: { ...workspaceRow, scopeKind: "holding", scopeId: "h003", label: "ERPX Holding", initials: "EH", documentMode: "holding" },
      branches: [workspaceRow],
      storefront: storefrontRow,
    });
    expect(calls[0].url.searchParams.get("kind")).toBe("profiles");
  });

  it("maps the native evidence row and keeps the retained null row", async () => {
    const row = { id: "branch-1", branch_name: "North Branch", document_mode: "both", updated_at: 1_700_000_000_000, primary_attachment_id: null, inverse_attachment_id: "att-2" };
    const { client, calls } = stub(() => row);
    expect(await client.evidence(actor, incoming, "branch-1")).toEqual(row);
    expect(calls[0].url.searchParams.get("kind")).toBe("evidence");
    expect(calls[0].url.searchParams.get("branch")).toBe("branch-1");
    expect(await stub(() => null).client.evidence(actor, incoming, "branch-1")).toBeNull();
  });

  it("fails closed on a malformed native payload instead of rendering a partial brand", async () => {
    const { client } = stub(() => ({ ...workspaceRow, accentColor: "red", documentMode: "elsewhere" }));
    await expect(client.workspace(actor, incoming)).rejects.toMatchObject({ code: "NATIVE_BRAND_PRESENTATION_RESPONSE_INVALID", status: 502 });
    const brokenStorefront = stub(() => ({ holding: workspaceRow, branches: [], storefront: { ...storefrontRow, accentColor: "blue" } }));
    await expect(brokenStorefront.client.profiles(actor, incoming)).rejects.toMatchObject({ code: "NATIVE_BRAND_PRESENTATION_RESPONSE_INVALID" });
  });

  it("keeps the native refusal code and status of a missing Company or Branch", async () => {
    await expect(failing(409, "COMPANY_REQUIRED").profiles(actor, incoming)).rejects.toMatchObject({ code: "COMPANY_REQUIRED", status: 409 });
    await expect(failing(404, "BRANCH_NOT_FOUND").document(actor, incoming, { branchId: "missing" })).rejects.toMatchObject({ code: "BRANCH_NOT_FOUND", status: 404 });
  });

  it("never falls back to the retained SQL read when the transport is down", async () => {
    const offline = createNativeBrandPresentationClient(config, async () => {
      throw new Error("socket closed");
    });
    await expect(offline.workspace(actor, incoming)).rejects.toMatchObject({ code: "NATIVE_BRAND_PRESENTATION_UNAVAILABLE", status: 503 });
    const foreign = createNativeBrandPresentationClient(config, async () => new Response(null, { status: 200 }));
    await expect(foreign.workspace(actor, incoming)).rejects.toMatchObject({ code: "NATIVE_BRAND_PRESENTATION_UNAVAILABLE" });
  });

  it("refuses a configured Holding or context the client cannot prove", async () => {
    const wrongSlug = { ...actor, holdingSlug: "acme" } as WorkContext;
    await expect(stub(() => workspaceRow).client.workspace(wrongSlug, incoming)).rejects.toMatchObject({ code: "NATIVE_BRAND_PRESENTATION_TARGET_UNAVAILABLE" });
    const noAuthority = { ...actor, authority: undefined } as WorkContext;
    await expect(stub(() => workspaceRow).client.workspace(noAuthority, incoming)).rejects.toMatchObject({ code: "WORK_CONTEXT_CHANGED", status: 409 });
  });

  it("keeps the native switch explicit and unset configurations fail closed", () => {
    expect(nativeBrandPresentationConfigured({})).toBe(false);
    expect(nativeBrandPresentationConfigured({ ERPX_NATIVE_BRANDING_CONFIG: config })).toBe(true);
    expect(() => createNativeBrandPresentationClient("not-json")).toThrowError();
    expect(() =>
      createNativeBrandPresentationClient(config.replace("http://127.0.0.1:3199", "http://0.0.0.0:3199")),
    ).toThrowError();
  });
});
