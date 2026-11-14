import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { legacyGuestAccountOperation } from "@erpx/feature-identity_access/lib/guest-account-compat";

// Applied schema history and dated evidence remain byte-for-byte historical.
const immutable = [/^migrations\/postgres\/(control|control-read|holding-read)\//,/^docs\/adr\/ADR-00[1-9]-/,/^docs\/adr\/ADR-0[12][0-9]-/,/^docs\/adr\/ADR-03[0-9]-/,/^docs\/archive\//,/^archive\//,/^docs\/testing\/evidence\//];
// These exact oracle files must retain old persisted receipt spellings to
// prove compatibility. This exception does not admit new business vocabulary.
const compatibility = new Set(["packages/feature-identity_access/src/lib/guest-account-compat.ts","migrations/postgres/holding-write/1026_guest_accounts_and_hospitality_boundaries.sql","fixtures/workflow/rust-commands/conformance.v1.json","scripts/testing/fixtures/generate-workflow-rust-oracle.ts"]);
const roots = /^(apps|packages|src|scripts|tests|fixtures|docs|Designed|public|migrations)\/|^CONTEXT\.md$/;
const textFile = /\.(tsx?|mts|cts|mjs|cjs|js|jsx|json|md|css|html|sql|txt|yaml|yml)$/;
const old = legacyGuestAccountOperation("guest_account.").slice(0,-1);
const forbidden = new RegExp(`\\b${old}|${old[0].toUpperCase()}${old.slice(1)}|\\b${old.toUpperCase()}|\u0e42\u0e1f\u0e25\u0e34\u0e42\u0e2d`);
const files = [...new Set(execFileSync("git",["ls-files","--cached","--others","--exclude-standard","-z"],{encoding:"utf8",maxBuffer:64*1024*1024}).split("\0").filter(Boolean))];
const failures: string[] = [];
for(const file of files) {
  if(!roots.test(file)||!textFile.test(file)||compatibility.has(file)||(/^migrations\/postgres\/holding-write\//.test(file) && Number(file.split("/").at(-1)?.slice(0,4))<=1025)||immutable.some(pattern=>pattern.test(file)))continue;
  try {
    const lines=readFileSync(file,"utf8").split("\n");
    lines.forEach((line,index)=>{if(forbidden.test(line))failures.push(`${file}:${index+1}`);});
  } catch(error) { if((error as NodeJS.ErrnoException).code!=="ENOENT")throw error; }
}
if(failures.length)throw new Error(`GUEST_ACCOUNT_TERMINOLOGY_REQUIRED\n${failures.join("\n")}`);
console.log("Guest Account terminology passed; compatibility and immutable history excluded explicitly.");
