import { writeFileSync } from "node:fs";

/** Retain byte-exact UTF-8 for large CLI inventories, including pipe/file output. */
export function writeJsonOutput(value: unknown): void {
  writeFileSync(1, JSON.stringify(value, null, 2) + "\n", "utf8");
}
