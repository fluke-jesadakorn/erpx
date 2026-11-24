/** Owner-local quality source projection shared by the purchased workspace
 * read guards. The three arms (hold, alert and specification) differ only in
 * their five comma-separated projections and their source relation; the
 * statement head and comma layout are byte-identical. The caller supplies
 * only fixed owner-owned fragments and binds remain unchanged. */
export const qualitySourceUnionSelect = (
  id: string,
  company: string,
  branch: string,
  department: string,
  kind: string,
  from: string,
) => `SELECT ${id},${company},${branch},${department},${kind} FROM ${from}`;
