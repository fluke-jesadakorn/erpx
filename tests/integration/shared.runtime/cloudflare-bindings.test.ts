import { env } from "../../support/runtime-env";
import { beforeEach, describe, expect, it } from "vitest";

describe("Cloudflare persistence invariants", () => {
  beforeEach(async () => {
    await env.H001_DB.exec(`
      DROP TABLE IF EXISTS test_command_assertions;
      DROP TABLE IF EXISTS test_records;
      CREATE TABLE test_records(id TEXT PRIMARY KEY, version BIGINT NOT NULL, marker TEXT);
      CREATE TABLE test_command_assertions(command_id TEXT PRIMARY KEY, ok BIGINT NOT NULL CHECK(ok=1));
      INSERT INTO test_records VALUES ('record-1', 1, NULL);
    `);
  });

  it("rolls an earlier guarded update back when its SQL assertion fails", async () => {
    const commandId = "stale-command";
    await expect(env.H001_DB.batch([
      env.H001_DB.prepare("UPDATE test_records SET version=2, marker=? WHERE id='record-1' AND version=99").bind(commandId),
      env.H001_DB.prepare(`INSERT INTO test_command_assertions(command_id,ok)
        VALUES(?, CASE WHEN EXISTS(SELECT 1 FROM test_records WHERE marker=? AND version=2) THEN 1 ELSE 0 END)`)
        .bind(commandId, commandId),
    ])).rejects.toThrow();

    const record = await env.H001_DB.prepare("SELECT version, marker FROM test_records WHERE id='record-1'")
      .first<{ version: number; marker: string | null }>();
    expect(record).toEqual({ version: 1, marker: null });
  });

  it("stores private R2 content with integrity metadata", async () => {
    const key = "h001/test/evidence.csv";
    await env.H001_FILES.put(key, "name,value\ncoverage,ok", {
      httpMetadata: { contentType: "text/csv" },
      customMetadata: { sha256: "fixture-checksum" },
    });

    const object = await env.H001_FILES.get(key);
    expect(object?.httpMetadata?.contentType).toBe("text/csv");
    expect(object?.customMetadata?.sha256).toBe("fixture-checksum");
    expect(await object?.text()).toContain("coverage,ok");
  });
});
