import { writeFileSync } from "node:fs";

export type ProductAcceptanceOutput = { path: string; contents: string };

/** Keep an invalid acceptance calculation from partially replacing its outputs. */
export function writeProductAcceptanceOutputs(
  errors: readonly string[],
  outputs: readonly ProductAcceptanceOutput[],
): void {
  if (errors.length) throw new Error(errors.join("\n"));
  for (const output of outputs) writeFileSync(output.path, output.contents);
}
