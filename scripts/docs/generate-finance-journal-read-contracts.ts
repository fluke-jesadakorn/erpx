import { existsSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

const root = resolve(import.meta.dir, "../..");
const arguments_ = process.argv.slice(2);
if (arguments_.some(value => value !== "--check") || arguments_.length > 1) throw new Error("FINANCE_JOURNAL_READ_CONTRACT_ARGUMENTS_INVALID");
const cargo = process.env.ERPX_CARGO_BIN ?? "cargo";
const command = [cargo, "run", "--quiet", "--locked", "--package", "erpx-service", "--example", "export-finance-journal-read-contracts", "--features", "contracts"];
const generated = Bun.spawnSync(command, { cwd: root, stdout: "pipe", stderr: "pipe" });
if (generated.exitCode !== 0) {
  process.stderr.write(generated.stderr);
  throw new Error("FINANCE_JOURNAL_READ_CONTRACT_GENERATION_FAILED");
}
const source = generated.stdout.toString();
if (!source.startsWith("// Generated from the Rust Finance journal read contracts.") || !source.includes("export type FinanceJournalReadResponse")) throw new Error("FINANCE_JOURNAL_READ_CONTRACT_OUTPUT_INVALID");
const openapi = Bun.spawnSync([...command, "--", "--openapi"], { cwd: root, stdout: "pipe", stderr: "pipe" });
if (openapi.exitCode !== 0) { process.stderr.write(openapi.stderr); throw new Error("FINANCE_JOURNAL_READ_OPENAPI_GENERATION_FAILED"); }
const specification = openapi.stdout.toString();
const document: unknown = JSON.parse(specification);
if (!document || typeof document !== "object" || !("openapi" in document) || !("paths" in document)) throw new Error("FINANCE_JOURNAL_READ_OPENAPI_OUTPUT_INVALID");
for (const [relative, content] of [["packages/contracts/src/finance-journal-read.generated.ts", source], ["docs/contracts/next/finance-journal-read.openapi.generated.json", specification]]) {
  const destination = resolve(root, relative);
  if (arguments_.includes("--check")) {
    if (!existsSync(destination) || readFileSync(destination, "utf8") !== content) throw new Error(`FINANCE_JOURNAL_READ_CONTRACTS_STALE:${relative}`);
  } else {
    const temporary = `${destination}.${process.pid}.tmp`;
    writeFileSync(temporary, content);
    renameSync(temporary, destination);
  }
}
console.log(arguments_.includes("--check") ? "Rust Finance journal read TypeScript and OpenAPI contracts match their sources." : "Generated Finance journal read TypeScript and OpenAPI contracts from Rust.");
