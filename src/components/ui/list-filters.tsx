import Link from "next/link";
import { Button, SearchField, Select } from "./foundation";
export function ListFilters({action,query="",placeholder="Search",status,statuses,preserve={}}:{action:string;query?:string;placeholder?:string;status?:string;statuses?:readonly (readonly [string,string])[];preserve?:Record<string,string>}) {
 return <form className="work-filters" action={action} role="search">
  {Object.entries(preserve).map(([name,value])=><input key={name} type="hidden" name={name} value={value}/>)}
  <SearchField name="q" aria-label={placeholder} placeholder={placeholder} defaultValue={query}/>
  {statuses&&<Select name="status" aria-label="Status" defaultValue={status??"all"}>{statuses.map(([id,label])=><option key={id} value={id}>{label}</option>)}</Select>}
  <Button type="submit" variant="secondary">Search</Button>
  {(query || status&&status!=="all")&&<Link className="button ghost" href={`${action}${Object.keys(preserve).length?`?${new URLSearchParams(preserve)}`:""}`}>Clear filters</Link>}
 </form>;
}
