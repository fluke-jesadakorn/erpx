import {
  afterAll,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import { Document, Packer, Paragraph } from "docx";
import { applyMigrations, env } from "./runtime-env";
const attachmentRuntime = vi.hoisted(() => ({
  scannerReady: true,
  previewReady: true,
}));
vi.mock("../src/server/runtime", () => ({
  holdingResources: async () => ({ write: env.H001_DB, files: env.H001_FILES }),
  runtimeEnv: async () => env,
}));
vi.mock("../src/server/attachments/scanner", () => ({
  attachmentScannerReadiness: async () => ({
    enabled: true,
    ready: attachmentRuntime.scannerReady,
    engine: "ClamAV specialized-adapter test",
    definitionUpdatedAt: Date.now(),
    error: null,
  }),
  scanAttachmentBytes: async () => ({
    clean: true,
    signature: null,
    engine: "ClamAV specialized-adapter test",
    definitionUpdatedAt: Date.now(),
  }),
}));
vi.mock("../src/server/attachments/preview", async () => {
  const actual = await vi.importActual<
    typeof import("../src/server/attachments/preview")
  >("../src/server/attachments/preview");
  return {
    ...actual,
    attachmentPreviewReadiness: async () => ({
      ready: attachmentRuntime.previewReady,
      heifConvert: "/test/heif-convert",
      pdftoppm: "/test/pdftoppm",
      error: null,
    }),
    sanitizedAttachmentPreview: async () => null,
  };
});
import {
  expenseDb as db,
  expenseFixture,
  key,
} from "./helpers/expense-fixture";
import {
  attachmentBindingState,
  seedAttachmentSet,
} from "./helpers/attachment-v2";
import {
  createMessagingTeam,
  messageSchema,
  messagingFile,
  messagingRoom,
  sendMessagingMessage,
} from "../src/server/messaging";
import {
  attachmentContent,
  bindRecordAttachmentSet,
} from "../src/server/attachments/service";
import {
  assertAttachmentMedia,
  attachmentPolicy,
} from "../src/server/attachments/policy";
import { detectedAttachmentMediaType } from "../src/server/attachments/preview";
import { getExpense } from "../src/server/expenses";
import { expenseRequirements } from "../src/server/expense-drafts";
import { legalCommand, detail as legalDetail } from "../src/server/legal/service";
import { access as legalAccess } from "../src/server/legal/access";
import { portalUpload } from "../src/server/legal/portal";
import {
  documentExport,
  externalSubmission,
  uploadDocument,
} from "../src/server/legal/documents";

let fixture: Awaited<ReturnType<typeof expenseFixture>>;

beforeAll(async () => {
  await applyMigrations(db, env.STAFF_TEST_MIGRATIONS);
  await applyMigrations(env.CONTROL_DB, env.CONTROL_TEST_MIGRATIONS);
  process.env.ERPX_ATTACHMENTS_V2 = "true";
});

afterAll(() => {
  delete process.env.ERPX_ATTACHMENTS_V2;
});

beforeEach(async () => {
  fixture = await expenseFixture(false);
  for (const actor of [
    fixture.claimant,
    fixture.head,
    fixture.owner,
    fixture.reviewer,
  ])
    await env.CONTROL_DB.prepare(
      'INSERT INTO "user"(id,name,email,emailVerified,createdAt,updatedAt) VALUES(?,?,?,1,1,1)',
    )
      .bind(actor.userId, actor.name, `${actor.userId}@example.invalid`)
      .run();
});

async function messageFile(
  roomId: string,
  sourceState: "available" | "quarantined",
  componentRef?: string,
) {
  const room = await messagingRoom(db, fixture.claimant, roomId);
  const fileId = key();
  const canonical = await seedAttachmentSet(db, fixture.claimant, {
    targetType: "messaging_room",
    targetId: roomId,
    componentKind: "message",
    componentRef: componentRef ?? fileId,
    purposeKey: "message_attachment",
    expectedTargetVersion: Number(room.version),
    sourceState,
    mediaType: "text/csv",
  });
  const source = await db
    .prepare(
      "SELECT object_key,checksum_sha256 FROM attachments WHERE id=?",
    )
    .bind(canonical.sourceId)
    .first<{ object_key: string; checksum_sha256: string }>();
  await db
    .prepare(
      `INSERT INTO messaging_files(
        id,room_id,created_by,original_name,media_type,bytes,object_key,checksum,created_at,
        attachment_set_id,attachment_link_id,attachment_source_id
      ) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)`,
    )
    .bind(
      fileId,
      roomId,
      fixture.claimant.userId,
      "evidence.csv",
      "text/csv",
      8,
      source!.object_key,
      source!.checksum_sha256,
      Date.now(),
      canonical.setId,
      canonical.linkId,
      canonical.sourceId,
    )
    .run();
  return { fileId, ...canonical, objectKey: source!.object_key };
}

describe("specialized canonical attachment adapters", () => {
  it("keeps Messaging CSV pending until clean, then binds it atomically and hides guessed links", async () => {
    const wrongContextTeam = await createMessagingTeam(
      db,
      fixture.claimant,
      { name: "Wrong context evidence", members: [fixture.head.userId] },
      key(),
    );
    const wrongContext = await messageFile(wrongContextTeam.roomId, "available", `another-file-${key()}`);
    await expect(
      sendMessagingMessage(
        db,
        fixture.claimant,
        wrongContextTeam.roomId,
        messageSchema.parse({ body: "", files: [wrongContext.fileId] }),
        key(),
      ),
    ).rejects.toMatchObject({ code: "ATTACHMENT_BINDING_CONTEXT" });
    expect(await attachmentBindingState(db, wrongContext.setId)).toMatchObject({ set_state: "draft", link_state: "draft" });

    const pendingTeam = await createMessagingTeam(
      db,
      fixture.claimant,
      { name: "Pending evidence", members: [fixture.head.userId] },
      key(),
    );
    const pending = await messageFile(pendingTeam.roomId, "quarantined");
    await expect(
      sendMessagingMessage(
        db,
        fixture.claimant,
        pendingTeam.roomId,
        messageSchema.parse({ body: "", files: [pending.fileId] }),
        key(),
      ),
    ).rejects.toMatchObject({ code: "ATTACHMENT_SCAN_PENDING" });
    expect((await attachmentBindingState(db, pending.setId))?.set_state).toBe(
      "draft",
    );

    const team = await createMessagingTeam(
      db,
      fixture.claimant,
      { name: "Clean evidence", members: [fixture.head.userId] },
      key(),
    );
    const clean = await messageFile(team.roomId, "available");
    await env.H001_FILES.put(clean.objectKey, "a,b\n1,2\n", {
      onlyIf: { etagDoesNotMatch: "*" },
      httpMetadata: { contentType: "text/csv" },
    });
    const sent = await sendMessagingMessage(
      db,
      fixture.claimant,
      team.roomId,
      messageSchema.parse({ body: "", files: [clean.fileId] }),
      key(),
    );
    expect(sent.id).toMatch(/^message-/);
    expect(await attachmentBindingState(db, clean.setId)).toMatchObject({
      set_state: "bound",
      link_state: "bound",
      binding_type: "message_attachment",
      binding_id: sent.id,
    });
    expect(Number((await messagingRoom(db, fixture.claimant, team.roomId)).version)).toBe(
      2,
    );
    expect((await messagingFile(db, fixture.head, clean.fileId)).id).toBe(
      clean.fileId,
    );
    const download = await attachmentContent(
      db,
      env.H001_FILES,
      fixture.head,
      clean.linkId,
      "original",
    );
    expect(await download.object.text()).toBe("a,b\n1,2\n");
    await expect(
      attachmentContent(
        db,
        env.H001_FILES,
        fixture.owner,
        clean.linkId,
        "original",
      ),
    ).rejects.toMatchObject({ status: 404 });
    await expect(
      attachmentContent(
        db,
        env.H001_FILES,
        fixture.head,
        `guessed-${key()}`,
        "original",
      ),
    ).rejects.toMatchObject({
      status: 404,
      code: "ATTACHMENT_LINK_NOT_FOUND",
    });
    expect(
      Number(
        await db
          .prepare(
            "SELECT count(*) FROM attachment_access_events WHERE link_id=? AND outcome='denied'",
          )
          .bind(clean.linkId)
          .first("count"),
      ),
    ).toBe(1);
  });

  it("counts only clean bound Expense CSV evidence", async () => {
    const claimId = await fixture.claim();
    const claim = await getExpense(fixture.claimant, claimId);
    const clean = await seedAttachmentSet(db, fixture.claimant, {
      targetType: "expense_claim",
      targetId: claimId,
      componentKind: "record",
      purposeKey: "receipt",
      expectedTargetVersion: Number(claim.version),
      mediaType: "text/csv",
    });
    expect((await expenseRequirements(db, claim)).join(" ")).toContain(
      "Attach the receipt",
    );
    await bindRecordAttachmentSet(
      db,
      fixture.claimant,
      clean.setId,
      Number(claim.version),
      key(),
    );
    const after = await getExpense(fixture.claimant, claimId);
    expect((await expenseRequirements(db, after)).join(" ")).not.toContain(
      "Attach the receipt",
    );

    const secondId = await fixture.claim();
    const second = await getExpense(fixture.claimant, secondId);
    const pending = await seedAttachmentSet(db, fixture.claimant, {
      targetType: "expense_claim",
      targetId: secondId,
      componentKind: "record",
      purposeKey: "receipt",
      expectedTargetVersion: Number(second.version),
      sourceState: "quarantined",
      mediaType: "text/csv",
    });
    await expect(
      bindRecordAttachmentSet(
        db,
        fixture.claimant,
        pending.setId,
        Number(second.version),
        key(),
      ),
    ).rejects.toMatchObject({ code: "ATTACHMENT_SCAN_PENDING" });
  });

  it("keeps CSV and DOCX as closed specialized media exceptions", async () => {
    const messaging = attachmentPolicy(
      "messaging_room",
      "message_attachment",
      "message",
    );
    const expense = attachmentPolicy("expense_claim", "receipt", "record");
    const legal = attachmentPolicy("legal_item", "legal_document", "record");
    const legalPortal = attachmentPolicy(
      "legal_item",
      "legal_portal_submission",
      "action",
    );
    expect(messaging.maxFiles).toBe(10);
    expect(() => assertAttachmentMedia(messaging, "text/csv", 8)).not.toThrow();
    expect(() => assertAttachmentMedia(expense, "text/csv", 8)).not.toThrow();

    const word = await Packer.toBuffer(
      new Document({ sections: [{ children: [new Paragraph("Evidence")] }] }),
    );
    const docx =
      "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
    expect(detectedAttachmentMediaType(word, docx)).toBe(docx);
    expect(() => assertAttachmentMedia(legal, docx, word.byteLength)).not.toThrow();
    expect(legalPortal.portalUpload).toBe(true);
    expect(() =>
      assertAttachmentMedia(legalPortal, docx, word.byteLength),
    ).not.toThrow();
    expect(() => assertAttachmentMedia(legalPortal, "text/csv", 8)).toThrow();
    expect(
      detectedAttachmentMediaType(new Uint8Array([0x50, 0x4b, 3, 4]), docx),
    ).toBeNull();
    expect(() =>
      assertAttachmentMedia(messaging, docx, word.byteLength),
    ).toThrow();
  });

  it("scans and binds staff and authenticated-portal Legal DOCX files", async () => {
    const manager = await fixture.member(
      `legal-adapter-${key()}`,
      "legal_manager",
    );
    await legalCommand(
      db,
      manager,
      { action: "settings", enabled: true },
      key(),
    );
    const created = await legalCommand(
      db,
      manager,
      {
        action: "create",
        item: {
          kind: "contract",
          title: "Portal evidence",
          ownerId: manager.userId,
          data: { contractType: "Supply", parties: "Company and supplier" },
        },
      },
      key(),
    );
    let item = await legalAccess(db, manager, String(created.id));
    await legalCommand(
      db,
      manager,
      {
        action: "document",
        id: created.id,
        expectedVersion: item.version,
        name: "Shared terms",
        body: "Terms shared with the authenticated counterparty.",
      },
      key(),
    );
    item = await legalAccess(db, manager, String(created.id));
    const internalWord = await Packer.toBuffer(
      new Document({
        sections: [{ children: [new Paragraph("Staff legal evidence")] }],
      }),
    );
    const mediaType =
      "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
    await uploadDocument(
      db,
      env.H001_FILES,
      manager,
      String(created.id),
      Number(item.version),
      "staff-evidence.docx",
      mediaType,
      internalWord,
      key(),
    );
    const internal = await db
      .prepare(
        "SELECT id,attachment_link_id,attachment_source_id FROM legal_documents WHERE item_id=? AND name='staff-evidence.docx'",
      )
      .bind(created.id)
      .first<{
        id: string;
        attachment_link_id: string;
        attachment_source_id: string;
      }>();
    expect(internal?.attachment_link_id).toBeTruthy();
    expect(
      await db
        .prepare("SELECT state FROM attachment_links WHERE id=?")
        .bind(internal!.attachment_link_id)
        .first("state"),
    ).toBe("bound");
    const staffDownload = await documentExport(
      db,
      env.H001_FILES,
      manager,
      internal!.id,
      null,
    );
    expect(staffDownload.headers.get("Content-Disposition")).toMatch(
      /^attachment;/,
    );
    expect(Buffer.from(await staffDownload.arrayBuffer())).toEqual(
      Buffer.from(internalWord),
    );

    const document = (
      await legalDetail(db, manager, String(created.id))
    ).documents.find((candidate) => candidate.name === "Shared terms")!;
    item = await legalAccess(db, manager, String(created.id));
    const email = `portal-${key()}@example.invalid`;
    await legalCommand(
      db,
      manager,
      {
        action: "share",
        id: created.id,
        expectedVersion: item.version,
        documentId: document.id,
        email,
        expiresAt: Date.now() + 86_400_000,
      },
      key(),
    );
    const share = await db
      .prepare(
        "SELECT id FROM legal_shares WHERE item_id=? AND lower(email)=? AND revoked_at IS NULL",
      )
      .bind(created.id, email)
      .first<{ id: string }>();
    const actor = {
      userId: `portal-user-${key()}`,
      email,
      sessionId: `portal-session-${key()}`,
    };
    const word = await Packer.toBuffer(
      new Document({
        sections: [{ children: [new Paragraph("Counterparty evidence")] }],
      }),
    );
    const command = key();
    const before = await legalAccess(db, manager, String(created.id));
    const previousGate = process.env.ERPX_ATTACHMENTS_V2;
    const previousReadOnly = process.env.ERPX_ATTACHMENTS_READ_ONLY;
    const previousFreeze = process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
    const sourceCount = Number(
      await db.prepare("SELECT count(*) FROM attachments").first("count"),
    );
    process.env.ERPX_ATTACHMENTS_V2 = "false";
    process.env.ERPX_ATTACHMENTS_READ_ONLY = "true";
    await expect(
      portalUpload(
        db,
        env.H001_FILES,
        actor,
        share!.id,
        "counterparty.docx",
        mediaType,
        word,
        key(),
      ),
    ).rejects.toMatchObject({ code: "ATTACHMENTS_READ_ONLY", status: 503 });
    delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
    process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE = "true";
    await expect(
      portalUpload(
        db,
        env.H001_FILES,
        actor,
        share!.id,
        "counterparty.docx",
        mediaType,
        word,
        key(),
      ),
    ).rejects.toMatchObject({ code: "ATTACHMENT_UPLOAD_FREEZE", status: 503 });
    delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
    attachmentRuntime.scannerReady = false;
    const unavailableCommand = key();
    await expect(
      portalUpload(
        db,
        env.H001_FILES,
        actor,
        share!.id,
        "counterparty.docx",
        mediaType,
        word,
        unavailableCommand,
      ),
    ).rejects.toMatchObject({
      code: "ATTACHMENT_SCANNER_UNAVAILABLE",
      status: 503,
    });
    expect(
      await env.H001_FILES.head(
        `legal/${fixture.claimant.companyId}/${created.id}/quarantine/attachment-upload-legal-portal-${unavailableCommand}/original`,
      ),
    ).toBeNull();
    attachmentRuntime.scannerReady = true;
    expect(
      Number(await db.prepare("SELECT count(*) FROM attachments").first("count")),
    ).toBe(sourceCount);
    let uploaded!: Awaited<ReturnType<typeof portalUpload>>;
    try {
      uploaded = await portalUpload(
        db,
        env.H001_FILES,
        actor,
        share!.id,
        "counterparty.docx",
        mediaType,
        word,
        command,
      );
      expect(
        await portalUpload(
          db,
          env.H001_FILES,
          actor,
          share!.id,
          "counterparty.docx",
          mediaType,
          word,
          command,
        ),
      ).toEqual(uploaded);
    } finally {
      attachmentRuntime.scannerReady = true;
      attachmentRuntime.previewReady = true;
      if (previousGate === undefined) delete process.env.ERPX_ATTACHMENTS_V2;
      else process.env.ERPX_ATTACHMENTS_V2 = previousGate;
      if (previousReadOnly === undefined)
        delete process.env.ERPX_ATTACHMENTS_READ_ONLY;
      else process.env.ERPX_ATTACHMENTS_READ_ONLY = previousReadOnly;
      if (previousFreeze === undefined)
        delete process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE;
      else process.env.ERPX_ATTACHMENT_UPLOAD_FREEZE = previousFreeze;
    }
    const row = await db
      .prepare(
        "SELECT attachment_link_id,attachment_source_id FROM legal_external_uploads WHERE id=?",
      )
      .bind(uploaded.id)
      .first<{
        attachment_link_id: string;
        attachment_source_id: string;
      }>();
    expect(row?.attachment_link_id).toBeTruthy();
    expect(
      await db
        .prepare("SELECT source_state FROM attachments WHERE id=?")
        .bind(row!.attachment_source_id)
        .first("source_state"),
    ).toBe("available");
    expect(
      await db
        .prepare("SELECT state FROM attachment_links WHERE id=?")
        .bind(row!.attachment_link_id)
        .first("state"),
    ).toBe("bound");
    expect(
      Number((await legalAccess(db, manager, String(created.id))).version),
    ).toBe(Number(before.version) + 1);
    const downloaded = await externalSubmission(
      db,
      env.H001_FILES,
      manager,
      String(uploaded.id),
    );
    expect(Buffer.from(downloaded.bytes)).toEqual(Buffer.from(word));
    expect(
      Number(
        await db
          .prepare(
            "SELECT count(*) FROM attachment_access_events WHERE link_id=? AND outcome='allowed' AND variant='original'",
          )
          .bind(row!.attachment_link_id)
          .first("count"),
      ),
    ).toBe(1);
  });
});
