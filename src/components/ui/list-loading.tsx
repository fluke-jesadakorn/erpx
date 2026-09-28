export function ListLoading() {
  return <div className="content-stack" aria-busy="true" aria-label="Loading results">
    <div className="card"><h1>Loading…</h1><p className="muted">Preparing this view</p></div>
    {Array.from({length:5},(_,index)=><div key={index} className="card" aria-hidden="true" style={{height:64,opacity:0.45}} />)}
  </div>;
}
