/** Expand room numbers before any writes, retaining zero padding and rejecting duplicates. */
export function expandRoomNumbers(input:string):string[] {
  const numbers=input.split(',').flatMap(part=>{
    const value=part.trim(),range=/^(\d+)\s*-\s*(\d+)$/.exec(value);
    if(!value) return [];
    if(!range) return [value];
    const from=Number(range[1]),to=Number(range[2]);
    if(to<from||to-from>199)throw new Error('Enter an ascending room range of at most 200 rooms.');
    return Array.from({length:to-from+1},(_,i)=>String(from+i).padStart(range[1].length,'0'));
  });
  if(!numbers.length||numbers.length>200)throw new Error('Enter between 1 and 200 room numbers.');
  if(new Set(numbers).size!==numbers.length)throw new Error('Duplicate room numbers. Review the grid before saving.');
  return numbers;
}
export type SetupSave = {key:string;requestKey?:string;url:string;body:Record<string,unknown>;label:string;state:'pending'|'saved'|'failed';error?:string;resultId?:string;dependsOn?:string;resolveHotelVersion?:boolean;rejected?:boolean};

export async function roomSaveKey(rowKey:string, number:string) {
  const digest=await crypto.subtle.digest('SHA-256',new TextEncoder().encode(number));
  return `${rowKey}:${Array.from(new Uint8Array(digest),b=>b.toString(16).padStart(2,'0')).join('').slice(0,32)}`;
}

/** Edit only requests the server definitively rejected; ambiguous responses must replay exactly. */
export function mergeSetupSave(queue:SetupSave[], candidate:SetupSave) {
  const existing=queue.find(j=>j.key===candidate.key);
  if(!existing){queue.push(candidate);return;}
  if(existing.state==='saved'||!existing.rejected&&!(existing.dependsOn&&!existing.body.productId))return;
  const body={...existing.body};
  if(candidate.dependsOn)delete body.productId;
  if(candidate.resolveHotelVersion)delete body.expectedVersion;
  if(JSON.stringify(body)===JSON.stringify(candidate.body))return;
  Object.assign(existing,candidate,{requestKey:crypto.randomUUID(),rejected:false,error:undefined});
}
