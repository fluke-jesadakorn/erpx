import "server-only";
/** Owner-local parameterized image-source row head shared by the Planning
 * attachment lineage sources. Every argument is a fixed server-owned column,
 * image-projection or FROM fragment; no bind value or caller SQL is
 * interpolated. Each emitted source is byte-identical to the former inline
 * literal it replaces. */
export const planningImageSourceRows = (columns: string, image: string, from: string) =>
  `SELECT ${columns},\n ${image} image\n FROM ${from}`;
