import Link from "next/link";
export function ListPagination({ nextCursor, previousCursor, query, count, cursorName="cursor" }: {
  cursorName?: string;
  nextCursor: string | null; previousCursor: string | null;
  query: Record<string, string | string[] | undefined>; count: number;
}) {
  const href = (cursor: string) => {
    const params = new URLSearchParams();
    for (const [key,value] of Object.entries(query)) if (typeof value === "string" && key !== cursorName) params.set(key,value);
    params.set(cursorName,cursor);
    return `?${params}`;
  };
  const first=new URLSearchParams();
  for(const [key,value] of Object.entries(query))if(typeof value==="string"&&key!==cursorName)first.set(key,value);
  return <nav className="form-actions" aria-label="Pagination">
    {query[cursorName]&&<Link className="button secondary" href={`?${first}`} prefetch={false}>First page</Link>}
    {previousCursor && <Link className="button secondary" href={href(previousCursor)} prefetch={false}>Previous</Link>}
    <span className="muted">{count} items on this page</span>
    {nextCursor && <Link className="button secondary" href={href(nextCursor)} prefetch={false}>Next</Link>}
  </nav>;
}
