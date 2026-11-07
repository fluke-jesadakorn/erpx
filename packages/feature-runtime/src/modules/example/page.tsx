// Optional example only: it exercises a versioned, read-only backend contract.
export const dynamic = 'force-dynamic';
export default async function ExampleModulePage() {
  const configured = process.env.ERPX_EXAMPLE_API_ORIGIN;
  let origin: URL;
  try {
    origin = new URL(configured ?? '');
    const port=Number(origin.port);
    if (origin.protocol !== 'http:' || !['127.0.0.1','[::1]'].includes(origin.hostname) || !origin.port || [3000,3001,3100,3101,3103,5432,5433,5435,8080].includes(port) || port>=3200&&port<=3299 || origin.origin !== configured || origin.username || origin.password) throw Error('invalid origin');
  } catch { throw Error('Example service is unavailable.'); }
  let response: Response;
  try { response = await fetch(new URL('/api/modules/example',origin), {cache:'no-store',redirect:'error',signal:AbortSignal.timeout(5000)}); }
  catch { throw Error('Example service is unavailable.'); }
  if (!response.ok || response.redirected || response.headers.get('content-type')?.split(';')[0]?.trim() !== 'application/json') throw Error('Example service is unavailable.');
  if(!response.body)throw Error('Example service is unavailable.');
  const reader=response.body.getReader(),chunks:Uint8Array[]=[];let size=0;
  try{for(;;){const part=await reader.read();if(part.done)break;size+=part.value.length;if(size>16384){await reader.cancel();throw Error('Example service is unavailable.');}chunks.push(part.value);}}finally{reader.releaseLock();}
  const bytes=new Uint8Array(size);let offset=0;for(const chunk of chunks){bytes.set(chunk,offset);offset+=chunk.length;}
  let value: unknown;try{value=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));}catch{throw Error('Example service is unavailable.');}
  if (!value || typeof value !== 'object' || !('acceptance' in value) || value.acceptance !== false || !('data' in value) || !value.data || typeof value.data !== 'object') throw Error('Example service is unavailable.');
  const data = value.data as Record<string,unknown>;
  if (data.moduleId !== 'erpx-example' || data.contract !== 'erpx.example.http' || data.contractVersion !== '1.0.0' || data.eventSchema !== 'erpx.example.observed' || data.eventSchemaVersion !== '1.0.0') throw Error('Example service is unavailable.');
  return <main><h1>Example module</h1><p>Connected to {String(data.contract)} version {String(data.contractVersion)}.</p></main>;
}
