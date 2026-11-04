import "server-only";
import { runtimeResourcePath } from "@erpx/server-runtime/public/runtime/resources";
import { mkdir, writeFile } from "node:fs/promises";
import { isAbsolute, join } from "node:path";
import { X509Certificate } from "node:crypto";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
const exec = promisify(execFile);
import {createSignedPdfValidation,type LegalTrustValidationMaterial} from "@erpx/server-legal/application/pdf-validation";
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
    // Native Rust owner verifier. The Python B0 validator exists only under
    // tests/support/legal/pdf-oracle and is never a runtime fallback.
    const verifier=process.env.ERPX_LEGAL_PDF_VERIFIER_BIN??runtimeResourcePath("bin/erpx-verify-pdf");
    if(!isAbsolute(verifier))throw new Error("LEGAL_PDF_VERIFIER_NOT_CONFIGURED");
    const { stdout } = await exec(
      verifier,
      [input, trust, new Date().toISOString()],
      {
        timeout: 90000,
        maxBuffer: 2 * 1024 * 1024,
        env: { NODE_ENV: process.env.NODE_ENV ?? "production", LANG: "C", TZ: "UTC" },
      },
    );
return JSON.parse(stdout) as Record<string,unknown>;
}})();}
