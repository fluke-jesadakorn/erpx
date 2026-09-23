"use client";

import { useState } from "react";
import { hospitalityFutureFeatures } from "@/data/hospitality-future-features";

export function FutureFeatures() {
  const [query, setQuery] = useState("");
  const shown = hospitalityFutureFeatures.filter((feature) =>
    `${feature.name} ${feature.value} ${feature.prerequisite}`.toLowerCase().includes(query.trim().toLowerCase()),
  );
  return <main style={{maxWidth:960,margin:"0 auto",padding:"24px 20px 80px"}}>
    <p style={{color:"var(--text-muted, #667085)"}}>ERPX · Product roadmap</p>
    <h1>Future features</h1>
    <p>These ideas are separate from daily hotel and restaurant work. “Coming Soon” and “Prototype” do not enable business commands or promise a launch date.</p>
    <label style={{display:"grid",gap:8,margin:"24px 0"}}>Search future features
      <input type="search" value={query} onChange={(event)=>setQuery(event.target.value)} placeholder="Search by feature or prerequisite" style={{width:"100%",maxWidth:520,padding:12,border:"1px solid #9aa4b2",borderRadius:8}}/>
    </label>
    <p aria-live="polite">{shown.length} feature{shown.length===1?"":"s"}</p>
    <div style={{display:"grid",gridTemplateColumns:"repeat(auto-fit,minmax(min(100%,280px),1fr))",gap:16}}>
      {shown.map((feature)=><article key={feature.id} style={{border:"1px solid #d0d5dd",borderRadius:12,padding:20}}>
        <p style={{fontSize:13,fontWeight:700,letterSpacing:1,textTransform:"uppercase"}}>{feature.stage}</p>
        <h2 style={{fontSize:20}}>{feature.name}</h2>
        <p>{feature.value}</p>
        <p><strong>Before release:</strong> {feature.prerequisite}</p>
      </article>)}
    </div>
    {!shown.length&&<p>No future feature matches this search.</p>}
  </main>;
}
