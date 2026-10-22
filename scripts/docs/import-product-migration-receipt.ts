import { resolve } from "node:path";
import { importProductMigrationReceipt } from "./product-migration-receipts";

const root = resolve(import.meta.dirname, "../..");
const args = process.argv.slice(2);
const inputs = args.filter(argument => argument.startsWith("--input="));
if (args.length !== 1 || inputs.length !== 1 || !inputs[0].slice("--input=".length)) throw new Error("PRODUCT_MIGRATION_IMPORT_INPUT_REQUIRED");

const result = await importProductMigrationReceipt(root, inputs[0].slice("--input=".length));
console.log(JSON.stringify(result));
