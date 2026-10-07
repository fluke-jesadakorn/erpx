import "server-only";
import { mkdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { X509Certificate } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
const exec = promisify(execFile);
import {createSignedPdfValidation,type LegalTrustValidationMaterial} from "../../application/pdf-validation";
export async function validateSignedPdf(db:SqlDatabase,companyId:string,input:string,directory:string){
 const trust=join(directory,"trust");
 for(const part of ["roots","intermediates","revocation"])await mkdir(join(trust,part),{recursive:true,mode:0o700});
 return createSignedPdfValidation({materials:async()=>(await db.prepare("SELECT id,kind,content,checksum FROM legal_trust_material WHERE company_id=? AND state='approved' ORDER BY id LIMIT 100").bind(companyId).all<LegalTrustValidationMaterial>()).results,prepare:async materials=>{
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
},validate:async()=>{
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
return JSON.parse(stdout) as Record<string,unknown>;
}})();}
