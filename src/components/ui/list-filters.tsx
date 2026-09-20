import Link from "next/link";
export function ListFilters({action,query="",placeholder="Search",status,statuses,preserve={}}:{action:string;query?:string;placeholder?:string;status?:string;statuses?:readonly (readonly [string,string])[];preserve?:Record<string,string>}) {
 return <form className="work-filters" action={action} role="search">
  {Object.entries(preserve).map(([name,value])=><input key={name} type="hidden" name={name} value={value}/>)}
  <input className="input" type="search" name="q" aria-label={placeholder} placeholder={placeholder} defaultValue={query}/>
  {statuses&&<select className="input" name="status" aria-label="Status" defaultValue={status??"all"}>{statuses.map(([id,label])=><option key={id} value={id}>{label}</option>)}</select>}
  <button className="button secondary">Search</button>
  {(query || status&&status!=="all")&&<Link className="button ghost" href={`${action}${Object.keys(preserve).length?`?${new URLSearchParams(preserve)}`:""}`}>Clear filters</Link>}
 </form>;
}
