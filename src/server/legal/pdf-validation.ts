import "server-only";
import { mkdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { X509Certificate } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import type { SqlDatabase } from "../runtime/types";
const exec = promisify(execFile);
export async function validateSignedPdf(
  db: SqlDatabase,
  companyId: string,
  input: string,
  directory: string,
) {
  const trust = join(directory, "trust");
  for (const d of ["roots", "intermediates", "revocation"])
    await mkdir(join(trust, d), { recursive: true, mode: 0o700 });
  const materials = (
    await db
      .prepare(
        "SELECT id,kind,content,checksum FROM legal_trust_material WHERE company_id=? AND state='approved' ORDER BY id LIMIT 100",
      )
      .bind(companyId)
      .all<{ id: string; kind: string; content: string; checksum: string }>()
  ).results;
  for (const [i, m] of materials.entries()) {
    const bytes = Buffer.from(m.content, "base64");
    if (m.kind === "root" || m.kind === "intermediate")
      await writeFile(
        join(trust, m.kind === "root" ? "roots" : "intermediates", `${i}.pem`),
        new X509Certificate(bytes).toString(),
        { mode: 0o600 },
      );
    else
      await writeFile(join(trust, "revocation", `${i}.${m.kind}`), bytes, {
        mode: 0o600,
      });
  }
  try {
    const { stdout } = await exec(
      process.env.ERPX_LEGAL_PYTHON ??
        resolve(".runtime/legal-python/bin/python"),
      [resolve("scripts/legal/validate_pdf.py"), input, trust],
      {
        timeout: 90000,
        maxBuffer: 2 * 1024 * 1024,
        env: { ...process.env, PYTHONDONTWRITEBYTECODE: "1" },
      },
    );
    const result = JSON.parse(stdout);
    if (!["valid", "invalid", "indeterminate"].includes(result.status))
      throw new Error("INVALID_VALIDATOR_RESPONSE");
    return {
      ...result,
      trustMaterials: materials.map(({ id, kind, checksum }) => ({
        id,
        kind,
        checksum,
      })),
    } as Record<string, unknown>;
  } catch {
    return {
      status: "indeterminate",
      reasons: ["VALIDATOR_UNAVAILABLE_OR_TIMED_OUT"],
      signatures: [],
      trustMaterials: materials.map(({ id, kind, checksum }) => ({
        id,
        kind,
        checksum,
      })),
    };
  }
}
