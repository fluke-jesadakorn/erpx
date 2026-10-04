import "server-only";
import { validateSignedPdf } from "./pdf-validation";
import { readFile, mkdtemp, writeFile, rm, readdir } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { z } from "zod";
import { Document, Paragraph, TextRun, Packer } from "docx";
import { PDFDocument } from "pdf-lib";
import fontkit from "@pdf-lib/fontkit";
import type { WorkContext } from "../authorization";
import type { SqlDatabase, FileStore } from "../runtime/types";
import { executedCopySchema, type ExecutedCopyInput, type DocumentVersion } from "@/lib/legal/model";
import { access, guards, event, fail } from "./access";
import { executeCommand, replayCommand, assertion } from "../commands";
import { hash } from "./service";
import { attachmentsV2ReadOnly } from "../attachments/feature";
import {
  attachmentContent,
  completeAttachmentUpload,
  createAttachmentSet,
  prepareAttachmentBinding,
  reserveAttachmentFile,
  scanAttachmentNow,
} from "../attachments/service";
const exec = promisify(execFile);
export async function uploadDocument(
  db: SqlDatabase,
  files: FileStore,
  a: WorkContext,
  id: string,
  version: number,
  name: string,
  type: string,
  bytes: Uint8Array,
  key: string,
  executed?: ExecutedCopyInput,
) {
  const r = await access(db, a, id, "legal.edit");
  if(executed)executed=executedCopySchema.parse(executed);
  if (executed ? r.kind!=="contract"||!["approved","executed","active"].includes(r.state) : r.state !== "draft") fail(executed?"Import a signed copy against an approved contract.":"Return to draft before adding a version.");
  z.string().min(1).max(200).parse(name);
  if (!bytes.length || bytes.length > 10 * 1024 * 1024)
    fail("Upload between 1 byte and 10 MB.");
  const allowed =
    (type === "application/pdf" &&
      Buffer.from(bytes.subarray(0, 5)).toString() === "%PDF-") ||
    (type === "image/png" &&
      Buffer.from(bytes.subarray(0, 8)).toString("hex") ===
        "89504e470d0a1a0a") ||
    (type === "image/jpeg" &&
      bytes[0] === 255 &&
      bytes[1] === 216 &&
      bytes[2] === 255) ||
    (type ===
      "application/vnd.openxmlformats-officedocument.wordprocessingml.document" &&
      bytes[0] === 80 &&
      bytes[1] === 75);
  if (!allowed)
    fail("Upload a PDF, PNG, JPEG or DOCX with a matching file signature.");
  const checksum = hash(bytes),
    input = { id, version, name, type, checksum, ...(executed?{executed}:{}) },
    operation=executed?"legal.executed.import":"legal.document.upload",
    prior = await replayCommand<{ id: string; version: number }>(
      db,
      a,
      key,
      operation,
      id,
      input,
    );
  if (prior) return prior;
  if (r.version !== version)
    fail("The record changed. Refresh first.", "LEGAL_VERSION_CONFLICT", 409);
  const approved=executed?await db.prepare("SELECT cycle,document_id,document_hash FROM legal_approvals WHERE item_id=? AND decision='approved' ORDER BY cycle DESC LIMIT 1").bind(id).first<{cycle:number;document_id:string;document_hash:string}>():null;
  if(executed&&(!approved||approved.document_id!==executed.approvedDocumentId))fail("Choose the current approved document.","LEGAL_APPROVAL_STALE",409);
  if(executed){
    if(type!=="application/pdf"&&!type.startsWith("image/"))fail("Import an executed copy as a PDF or scanned image.");
    if(executed.signedDate>new Intl.DateTimeFormat("en-CA",{timeZone:r.timezone,year:"numeric",month:"2-digit",day:"2-digit"}).format(Date.now()))fail("The signing date cannot be in the future.");
    for(const documentId of new Set(executed.signers.flatMap(s=>s.authorityDocumentIds))){
      const d=await documentAccess(db,a,documentId);
      if(d.item_id!==id)fail("Authority evidence must be attached to this contract.");
    }
  }
  const executedCopyId=executed?crypto.randomUUID():null;
  const executionSteps=(documentId:string)=>executed&&approved&&executedCopyId?[
    assertion(db,key,"legal-executed-approval","EXISTS(SELECT 1 FROM legal_approvals WHERE item_id=? AND cycle=? AND document_id=? AND document_hash=? AND decision='approved') AND NOT EXISTS(SELECT 1 FROM legal_approvals WHERE item_id=? AND cycle>?)",[id,approved.cycle,approved.document_id,approved.document_hash,id,approved.cycle]),
    ...Array.from(new Set(executed.signers.flatMap(s=>s.authorityDocumentIds))).map(d=>assertion(db,key,`legal-executed-authority-${d}`,"EXISTS(SELECT 1 FROM legal_documents WHERE id=? AND item_id=?) AND NOT EXISTS(SELECT 1 FROM legal_disposals WHERE document_id=?)",[d,id,d])),
    db.prepare("INSERT INTO legal_executed_copies(id,item_id,document_id,approved_cycle,approved_document_id,approved_hash,executed_hash,metadata,imported_by,imported_at) VALUES(?,?,?,?,?,?,?,?::jsonb,?,?)").bind(executedCopyId,id,documentId,approved.cycle,approved.document_id,approved.document_hash,checksum,JSON.stringify(executed),a.userId,Date.now()),
    db.prepare("UPDATE legal_documents SET locked=true WHERE id=?").bind(documentId),
  ]:[];
  if(attachmentsV2ReadOnly())fail("Attachment intake is disabled during rollback.","ATTACHMENTS_READ_ONLY",503);
  if (!attachmentsV2ReadOnly()) {
    const set = await createAttachmentSet(
      db,
      a,
      {
        targetType: "legal_item",
        targetId: id,
        componentKind: "record",
        componentRef: key,
        purposeKey: "legal_document",
        expectedTargetVersion: version,
      },
      `${key}:set`,
      { compatibilityAdapter: true },
    );
    const reserved = await reserveAttachmentFile(
      db,
      a,
      set.id,
      { filename: name, mediaType: type, bytes: bytes.byteLength },
      `${key}:file`,
      { compatibilityAdapter: true },
    );
    const uploaded = await completeAttachmentUpload(
      db,
      files,
      a,
      reserved.uploadIntentId,
      Uint8Array.from(bytes).buffer,
      { compatibilityAdapter: true },
    );
    const sourceState = await scanAttachmentNow(db, files, uploaded.sourceId);
    if (sourceState !== "available")
      fail(
        sourceState === "malware_rejected"
          ? "The upload was rejected by malware scanning."
          : "The upload could not pass malware scanning.",
        sourceState === "malware_rejected"
          ? "LEGAL_UPLOAD_REJECTED"
          : "LEGAL_SCAN_UNAVAILABLE",
        sourceState === "malware_rejected" ? 422 : 503,
      );
    const source = await db
      .prepare(
        "SELECT object_key,checksum_sha256,media_type,bytes FROM attachments WHERE id=? AND company_id=? AND source_state='available'",
      )
      .bind(uploaded.sourceId, a.companyId)
      .first<{
        object_key: string;
        checksum_sha256: string;
        media_type: string;
        bytes: number;
      }>();
    if (!source || source.checksum_sha256 !== checksum)
      fail("Stored file integrity check failed.", "LEGAL_INTEGRITY", 409);
    const documentId = crypto.randomUUID();
    const binding = await prepareAttachmentBinding(db, a, {
      attachmentSetIds: [set.id],
      bindingContract: {
        targetType: "legal_item",
        bindingType: "legal_document",
        contexts: [{ purposeKey: "legal_document", componentKind: "record", componentRef: key }],
      },
      targetId: id,
      expectedTargetVersion: version,
      commandId: key,
      bindingId: documentId,
    },{compatibilityAdapter:true});
    const next = (await db
      .prepare(
        "SELECT coalesce(max(version),0)+1 n FROM legal_documents WHERE item_id=?",
      )
      .bind(id)
      .first<{ n: number }>())!.n;
    return executeCommand(
      db,
      a,
      key,
      operation,
      input,
      id,
      version + 1,
      { id, version: version + 1, documentId, ...(executedCopyId?{executedCopyId,state:r.state}:{}) },
      [
        ...guards(db, a, id, version, key, true,false,["rule","template"].includes(r.kind)),
        ...binding.guards,
        db
          .prepare(
            "INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,object_key,created_by,created_at,attachment_link_id,attachment_source_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)",
          )
          .bind(
            documentId,
            id,
            next,
            name,
            source.media_type,
            source.checksum_sha256,
            Number(source.bytes),
            source.object_key,
            a.userId,
            Date.now(),
            uploaded.attachmentLinkId,
            uploaded.sourceId,
          ),
        db
          .prepare(
            "UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?",
          )
          .bind(Date.now(), id),
        ...binding.steps,
        ...executionSteps(documentId),
        event(
          db,
          a,
          id,
          executed?"executed.import":"document.upload",
          `${name} · ${checksum}`,
          version + 1,
        ),
      ],
    );
  }
  const objectKey = `legal/${a.companyId}/${id}/${key}/${checksum}`;
  await files.put(objectKey, bytes, {
    onlyIf: { etagDoesNotMatch: "*" },
    httpMetadata: { contentType: type },
    customMetadata: { legalItem: id, companyId: a.companyId, checksum },
  });
  const stored = await files.head(objectKey);
  if (stored?.etag !== checksum) fail("Stored file integrity check failed.");
  const next = (await db
    .prepare(
      "SELECT coalesce(max(version),0)+1 n FROM legal_documents WHERE item_id=?",
    )
    .bind(id)
    .first<{ n: number }>())!.n;
  return executeCommand(
    db,
    a,
    key,
    "legal.document.upload",
    input,
    id,
    version + 1,
    { id, version: version + 1 },
    [
      ...guards(db, a, id, version, key, true,false,["rule","template"].includes(r.kind)),
      db
        .prepare(
          "INSERT INTO legal_documents(id,item_id,version,name,media_type,checksum,bytes,object_key,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(
          crypto.randomUUID(),
          id,
          next,
          name,
          type,
          checksum,
          bytes.length,
          objectKey,
          a.userId,
          Date.now(),
        ),
      db
        .prepare(
          "UPDATE legal_items SET version=version+1,updated_at=? WHERE id=?",
        )
        .bind(Date.now(), id),
      event(db, a, id, "document.upload", `${name} · ${checksum}`, version + 1),
    ],
  );
}
export async function importExecutedCopy(db:SqlDatabase,files:FileStore,a:WorkContext,id:string,version:number,metadata:unknown,name:string,type:string,bytes:Uint8Array,key:string){
  return uploadDocument(db,files,a,id,version,name,type,bytes,key,executedCopySchema.parse(metadata));
}
export async function documentAccess(
  db: SqlDatabase,
  a: WorkContext,
  id: string,
  permission = "legal.read",
) {
  const d = await db
    .prepare("SELECT * FROM legal_documents WHERE id=?")
    .bind(id)
    .first<DocumentVersion>();
  if (!d) fail("Document unavailable.", "LEGAL_NOT_FOUND", 404);
  await access(db, a, d.item_id, permission);
  if (
    await db
      .prepare("SELECT 1 FROM legal_disposals WHERE document_id=?")
      .bind(id)
      .first()
  )
    fail(
      "Document content was destroyed under an approved retention policy. Metadata and destruction evidence remain.",
      "LEGAL_DOCUMENT_DISPOSED",
      410,
    );
  if(await db.prepare("SELECT 1 FROM legal_destructions WHERE item_id=? AND state='executing'").bind(d.item_id).first())
    fail("Document retention is in progress. Review or resume the preserved manifest before reading content.","LEGAL_RETENTION_FROZEN",409);
  return d;
}
export async function documentBytes(files: FileStore, d: DocumentVersion) {
  if (!d.object_key) {
    const bytes = Buffer.from(d.body);
    if (hash(bytes) !== d.checksum)
      fail("Document integrity check failed.", "LEGAL_INTEGRITY", 409);
    return bytes;
  }
  const file = await files.get(d.object_key);
  if (!file)
    fail("Document storage is unavailable.", "LEGAL_STORAGE_UNAVAILABLE", 503);
  const bytes = new Uint8Array(await file.arrayBuffer());
  if (hash(bytes) !== d.checksum)
    fail("Document integrity check failed.", "LEGAL_INTEGRITY", 409);
  return bytes;
}
async function authorizedDocumentBytes(
  db: SqlDatabase,
  files: FileStore,
  a: WorkContext,
  d: DocumentVersion,
) {
  if (d.attachment_link_id) {
    const canonical = await attachmentContent(
      db,
      files,
      a,
      d.attachment_link_id,
      "original",
      {compatibilityAdapter:true},
    );
    const bytes = new Uint8Array(await canonical.object.arrayBuffer());
    if (hash(bytes) !== d.checksum)
      fail("Document integrity check failed.", "LEGAL_INTEGRITY", 409);
    return bytes;
  }
  if (d.object_key)
    fail(
      "This document has not been migrated to the current attachment lifecycle.",
      "LEGAL_ATTACHMENT_LINK_REQUIRED",
      404,
    );
  return documentBytes(files, d);
}
export async function documentExport(
  db: SqlDatabase,
  files: FileStore,
  a: WorkContext,
  id: string,
  format: string | null,
) {
  const d = await documentAccess(db, a, id, "legal.export");
  let bytes: Uint8Array = await authorizedDocumentBytes(db, files, a, d);
  let type = d.media_type,
    name = d.name;
  if (!d.object_key && format === "docx") {
    bytes = await Packer.toBuffer(
      new Document({
        sections: [
          {
            children: d.body.split("\n").map(
              (t) =>
                new Paragraph({
                  children: [
                    new TextRun({
                      text: t,
                      font: "Noto Sans Thai",
                      size: 24,
                    }),
                  ],
                  spacing: { after: 160 },
                }),
            ),
          },
        ],
      }),
    );
    type =
      "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
    name += ".docx";
  } else if (!d.object_key && format === "pdf") {
    const pdf = await PDFDocument.create();
    pdf.registerFontkit(fontkit);
    const font = await pdf.embedFont(
      await readFile(join(process.cwd(), "public/fonts/NotoSansThai.ttf")),
      { subset: true },
    );
    let page = pdf.addPage([595, 842]),
      y = 790;
    for (const paragraph of d.body.split("\n")) {
      let line = "";
      for (const char of Array.from(paragraph)) {
        if (line && font.widthOfTextAtSize(line + char, 11) > 491) {
          page.drawText(line, { x: 52, y, size: 11, font });
          y -= 19;
          line = "";
          if (y < 55) {
            page = pdf.addPage([595, 842]);
            y = 790;
          }
        }
        line += char;
      }
      if (line) page.drawText(line, { x: 52, y, size: 11, font });
      y -= 22;
      if (y < 55) {
        page = pdf.addPage([595, 842]);
        y = 790;
      }
    }
    bytes = await pdf.save();
    type = "application/pdf";
    name += ".pdf";
  } else if (!d.object_key) name += ".txt";
  await db
    .prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)")
    .bind(
      crypto.randomUUID(),
      d.item_id,
      d.id,
      a.userId,
      `document.export.${format ?? "original"}`,
      Date.now(),
    )
    .run();
  return new Response(new Uint8Array(bytes), {
    headers: {
      "Content-Type": type,
      "Content-Disposition": `attachment; filename="document"; filename*=UTF-8''${encodeURIComponent(name)}`,
      "Cache-Control": "private, no-store",
      "X-Content-Type-Options": "nosniff",
    },
  });
}
export async function inspectDocument(
  db: SqlDatabase,
  files: FileStore,
  a: WorkContext,
  id: string,
  kind: "ocr" | "verify",
) {
  const d = await documentAccess(db, a, id),
    bytes = await authorizedDocumentBytes(db, files, a, d),
    directory = await mkdtemp(join(tmpdir(), "erpx-legal-"));
  try {
    const path = join(directory, "input");
    await writeFile(path, bytes, { mode: 0o600 });
    let result: Record<string, unknown>;
    if (kind === "verify") {
      if (d.media_type !== "application/pdf")
        fail("Signature inspection requires a PDF.");
      result = {
        ...(await validateSignedPdf(db, a.companyId, path, directory)),
        checksum: d.checksum,
      };
    } else {
      let text = d.body;
      let pageTexts:{page:number;text:string}[]=[];
      let pageMapping=d.media_type!=="application/vnd.openxmlformats-officedocument.wordprocessingml.document";
      if (d.object_key) {
        if (d.media_type === "application/pdf"){
          text = (
            await exec(
              process.env.ERPX_PDFTOTEXT_BIN ?? "/opt/homebrew/bin/pdftotext",
              ["-layout", path, "-"],
              { timeout: 30000, maxBuffer: 2 * 1024 * 1024 },
            )
          ).stdout;
          const segments=text.split("\f");
          if(!segments[segments.length-1]?.trim())segments.pop();
          pageTexts=segments.map((text,i)=>({page:i+1,text:text.slice(0,200000)}));
        }
        else if (d.media_type.startsWith("image/"))
          text = (
            await exec(
              process.env.ERPX_TESSERACT_BIN ?? "/opt/homebrew/bin/tesseract",
              [
                path,
                "stdout",
                "-l",
                process.env.ERPX_OCR_LANGUAGES ?? "eng+tha",
                "--tessdata-dir",
                process.env.ERPX_TESSDATA_DIR ??
                  resolve(".runtime/legal-ocr/tessdata"),
              ],
              { timeout: 45000, maxBuffer: 2 * 1024 * 1024 },
            )
          ).stdout;
        else if (
          d.media_type ===
          "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        )
          text = (
            await exec(
              process.execPath,
              [
                "--max-old-space-size=256",
                resolve("scripts/legal/extract_docx.mjs"),
                path,
              ],
              { timeout: 30000, maxBuffer: 1024 * 1024 },
            )
          ).stdout;
        else fail("Unsupported text extraction format.");
        if (d.media_type === "application/pdf" && !text.trim()) {
          const info = (
            await exec(
              process.env.ERPX_PDFINFO_BIN ?? "/opt/homebrew/bin/pdfinfo",
              [path],
              { timeout: 15000, maxBuffer: 100000 },
            )
          ).stdout;
          const pages = Number(info.match(/^Pages:\s+(\d+)/m)?.[1]);
          if (!pages || pages > 25)
            fail("Scanned PDFs are limited to 25 pages per document.");
          await exec(
            process.env.ERPX_PDFTOPPM_BIN ?? "/opt/homebrew/bin/pdftoppm",
            [
              "-png",
              "-scale-to",
              "1600",
              "-f",
              "1",
              "-l",
              String(pages),
              path,
              join(directory, "page"),
            ],
            { timeout: 90000, maxBuffer: 100000 },
          );
          const texts = [];
          for (const page of (await readdir(directory))
            .filter((n) => /^page-.*\.png$/.test(n))
            .sort())
            texts.push(
              (
                await exec(
                  process.env.ERPX_TESSERACT_BIN ??
                    "/opt/homebrew/bin/tesseract",
                  [
                    join(directory, page),
                    "stdout",
                    "-l",
                    process.env.ERPX_OCR_LANGUAGES ?? "eng+tha",
                    "--tessdata-dir",
                    process.env.ERPX_TESSDATA_DIR ??
                      resolve(".runtime/legal-ocr/tessdata"),
                  ],
                  { timeout: 20000, maxBuffer: 1000000 },
                )
              ).stdout,
            );
          text = texts.join("\n\n");
          pageTexts=texts.map((text,i)=>({page:i+1,text:text.slice(0,200000)}));
        }
      }
      if(!pageTexts.length)pageTexts=[{page:1,text:text.slice(0,200000)}];
      if(!d.object_key)pageMapping=false;
      let remaining=200000;
      pageTexts=pageTexts.map(p=>{const text=p.text.slice(0,remaining);remaining-=text.length;return {...p,text};});
      result = {
        text: text.slice(0, 200000),
        pages:pageTexts,
        pageMapping,
        truncated:text.length>200000,
        checksum: d.checksum,
        requiresReview: true,
      };
    }
    await documentAccess(db, a, id);
    const inspectionId=crypto.randomUUID();
    result={...result,inspectionId};
    const persisted=await db.batch([
      db.prepare(
        "SELECT pg_advisory_xact_lock(hashtextextended('legal-files',0))",
      ),
      db
        .prepare(
          "INSERT INTO legal_document_inspections SELECT ?,?,?,?,?,? WHERE NOT EXISTS(SELECT 1 FROM legal_destructions WHERE item_id=? AND state IN ('executing','completed'))",
        )
        .bind(
          inspectionId,
          d.id,
          kind,
          JSON.stringify(result),
          a.userId,
          Date.now(),
          d.item_id,
        ),
    ]);
    if(persisted[1].meta.changes!==1)fail("Document preservation changed while inspecting. Refresh first.","LEGAL_DOCUMENT_FROZEN",409);
    await db
      .prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)")
      .bind(
        crypto.randomUUID(),
        d.item_id,
        d.id,
        a.userId,
        `document.${kind}`,
        Date.now(),
      )
      .run();
    return result;
  } catch (e) {
    if (
      e instanceof Error &&
      "code" in e &&
      ["ENOENT", "ETIMEDOUT"].includes(String(e.code))
    )
      fail(
        "The document tool is unavailable or timed out.",
        "LEGAL_TOOL_UNAVAILABLE",
        503,
      );
    throw e;
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

export async function externalSubmission(
  db: SqlDatabase,
  files: FileStore,
  a: WorkContext,
  id: string,
) {
  const row = await db
    .prepare(
      "SELECT u.*,s.item_id FROM legal_external_uploads u JOIN legal_shares s ON s.id=u.share_id WHERE u.id=?",
    )
    .bind(id)
    .first<{
      id: string;
      item_id: string;
      object_key: string;
      checksum: string;
      name: string;
      media_type: string;
      bytes: number;
      attachment_link_id: string | null;
      attachment_source_id: string | null;
    }>();
  if (!row) fail("Submission unavailable.", "LEGAL_NOT_FOUND", 404);
  await access(db, a, row.item_id, "legal.export");
  if (!row.attachment_link_id)
    fail(
      "This submission has not been migrated to the current attachment lifecycle.",
      "LEGAL_ATTACHMENT_LINK_REQUIRED",
      404,
    );
  const canonical =
    row.attachment_link_id
      ? await attachmentContent(
          db,
          files,
          a,
          row.attachment_link_id,
          "original",
          {compatibilityAdapter:true},
        )
      : null;
  const f = canonical?.object ?? (await files.get(row.object_key));
  if (!f) fail("File unavailable.", "LEGAL_STORAGE_UNAVAILABLE", 503);
  const bytes = new Uint8Array(await f.arrayBuffer());
  if (hash(bytes) !== row.checksum) fail("File integrity check failed.");
  await db
    .prepare("INSERT INTO legal_access_events VALUES(?,?,?,?,?,?)")
    .bind(
      crypto.randomUUID(),
      row.item_id,
      null,
      a.userId,
      "external-submission.download",
      Date.now(),
    )
    .run();
  return { row, bytes };
}
