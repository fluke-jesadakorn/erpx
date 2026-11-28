import { evidenceHead } from "./inspection-evidence-source-sql";

/** Native projection wrapper shared by the Quality/Ecommerce settlement source reads. */
export const nativeSourceWhere = (source: string, predicate: string) => evidenceHead('native.*', ` FROM (${source}) native`, ` WHERE ${predicate}`);
