/** Read-only development audit. File bytes and credentials never enter stdout. */
import { getRuntime, closeRuntime } from "../../src/server/runtime/env";
import { resourceSlots, registeredResources } from "../../src/lib/holding-registry";
import { reconcileAttachmentStorage } from "../../src/server/attachments/migration";

const env = getRuntime();
if (env.ERPX_ENVIRONMENT !== "local") throw new Error("DEVELOPMENT_FILE_AUDIT_ONLY");
try {
  const result = [];
  for (const slot of resourceSlots(env)) {
    const { write, files } = registeredResources(env, slot.id);
    const database = await write.prepare("SELECT current_database() name").first<{ name: string }>();
    if (!database || !/^erpx_(?:dev|browser)_/.test(database.name)) throw new Error("DEVELOPMENT_DATABASE_REQUIRED");
    const report = await reconcileAttachmentStorage(write, files, { verifyChecksums: true });
    const columns = (await write.prepare(`SELECT table_name,column_name FROM information_schema.columns
      WHERE table_schema='public' AND data_type IN ('text','character varying')
        AND column_name IN ('object_key','preview_object_key','thumbnail_key','file_key','staging_key','quarantine_key','final_key')
      ORDER BY table_name,column_name`).all<{ table_name: string; column_name: string }>()).results;
    const provenance = [];
    for (const objectKey of report.orphanObjects) {
      const references: Array<{ table: string; column: string; rows: number }> = [];
      for (const column of columns) {
        if (!/^[a-z_]+$/.test(column.table_name) || !/^[a-z_]+$/.test(column.column_name)) throw new Error("UNEXPECTED_SCHEMA_IDENTIFIER");
        const count = await write.prepare(`SELECT COUNT(*)::int n FROM "${column.table_name}" WHERE "${column.column_name}"=?`).bind(objectKey).first<{ n: number }>();
        if (count?.n) references.push({ table: column.table_name, column: column.column_name, rows: count.n });
      }
      const head=await files.head(objectKey);
      const oldUpload=objectKey.match(/^[^/]+\/objects\/([^/]+)\/r(\d+)(?:\/|$)/);
      const upload=oldUpload ? await write.prepare("SELECT id,owner_type,owner_id,purpose,company_id,status,revision,checksum_sha256,actual_bytes FROM upload_intents WHERE id=?")
        .bind(oldUpload[1]).first() : null;
      provenance.push({ objectKey, references, objectSha256:head?.etag, bytes:head?.size,
        metadataKeys:Object.keys(head?.customMetadata??{}), legacyUpload:upload });
    }
    result.push({ holdingId: slot.id, database: database.name, orphanCount: report.orphanObjects.length, provenance });
  }
  console.log(JSON.stringify({ mode: "read-only", evidence: "exact persisted key references; no ownership inferred from filenames", holdings: result }, null, 2));
} finally { await closeRuntime(); }
