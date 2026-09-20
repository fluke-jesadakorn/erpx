import Link from "next/link";
export type SetupItem={id:string;label:string;ready:boolean;owner:string;href:string;detail?:string};
export function SetupChecklist({title="Readiness",items}:{title?:string;items:SetupItem[]}) {
 const missing=items.filter(item=>!item.ready).length;
 return <section className="card setup-checklist" aria-label={title}><header className="section-head"><h2>{title}</h2><span className={`badge ${missing?"warning":"success"}`}>{missing?`${missing} to prepare`:"Ready"}</span></header><ul>{items.map(item=><li key={item.id}><span aria-hidden>{item.ready?"✓":"○"}</span><div><strong>{item.label}</strong><p className="muted">{item.owner}{item.detail?` · ${item.detail}`:""}</p></div><Link href={item.href}>{item.ready?"Review":"Set up"}</Link></li>)}</ul></section>;
}
