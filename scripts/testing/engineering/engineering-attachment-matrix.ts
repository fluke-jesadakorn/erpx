import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { attachmentPolicyRegistry } from "@erpx/server-files/public/attachments/policy";
import { engineeringCaseSchema } from "@erpx/feature-runtime/lib/engineering-completion";

const path=resolve("tests/catalogue/engineering-attachment-owner-cases.json");
const policies=attachmentPolicyRegistry.flatMap(policy=>policy.componentKinds.map(componentKind=>({
  id:`${policy.targetType}/${policy.purposeKey}/${componentKind}`,targetType:policy.targetType,purposeKey:policy.purposeKey,componentKind,
}))).sort((a,b)=>a.id.localeCompare(b.id));
if(!existsSync(path))await writeFile(path,JSON.stringify(policies.map(policy=>({...policy,review:"pending",proof:"Awaiting actual authorized intake, scan, owner binding, reload and revoked/cross-Company assertions.",cases:[]})),null,2)+"\n");
const matrix=JSON.parse(await readFile(path,"utf8")) as Array<{id:string;review:string;proof:string;cases:unknown[]}>;
if(matrix.length!==policies.length||new Set(matrix.map(item=>item.id)).size!==policies.length||policies.some(policy=>!matrix.some(item=>item.id===policy.id)))throw new Error("ATTACHMENT_OWNER_MATRIX_INVENTORY_MISMATCH");
for(const item of matrix)for(const test of item.cases)engineeringCaseSchema.parse(test);
console.log(JSON.stringify({policies:attachmentPolicyRegistry.length,ownerPurposeComponents:policies.length,inventorySha256:createHash("sha256").update(policies.map(item=>item.id).join("\n")).digest("hex"),reviewed:matrix.filter(item=>item.review==="reviewed"&&item.cases.length).length,pending:matrix.filter(item=>item.review!=="reviewed"||!item.cases.length).length}));
