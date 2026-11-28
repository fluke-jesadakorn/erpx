import "server-only";
import { evidenceHead } from "./inspection-evidence-source-sql";
/** Owner-local full-row source read shared by the Quality settlement package
 * sources. The head routes through evidenceHead so this family keeps no inline
 * SELECT; the generated SQL stays byte-identical to the former inline full-row
 * reads for the same table argument. No bind value or caller SQL is
 * interpolated: table is a fixed owner-owned table name. */
export const qualitySourceRows = (table: string) => evidenceHead("*", ` FROM ${table}`, "");
