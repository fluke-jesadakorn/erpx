import { ActionLink } from "./foundation";
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
    {query[cursorName]&&<ActionLink href={`?${first}`} prefetch={false}>First page</ActionLink>}
    {previousCursor && <ActionLink href={href(previousCursor)} prefetch={false}>Previous</ActionLink>}
    <span className="muted" aria-live="polite">{count} items on this page</span>
    {nextCursor && <ActionLink href={href(nextCursor)} prefetch={false}>Next</ActionLink>}
  </nav>;
}
