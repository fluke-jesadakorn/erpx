export function assertHospitalityDevelopmentTarget(manifest:{databases:Record<string,string>;slots?:{beta?:boolean}[]},environment:string|undefined) {
  if(environment==='production'||environment==='staging'||manifest.slots?.some(s=>s.beta))throw new Error('HOSPITALITY_DEVELOPMENT_ONLY');
  if(!Object.keys(manifest.databases).length)throw new Error('HOSPITALITY_DEVELOPMENT_ONLY');
  for(const value of Object.values(manifest.databases)){
    const url=new URL(value);
    if(!['postgres:','postgresql:'].includes(url.protocol)||!['127.0.0.1','localhost','[::1]'].includes(url.hostname)||(url.port||'5432')!=='5432'||!/^\/erpx_dev_[a-z0-9_]+$/.test(url.pathname))throw new Error('HOSPITALITY_DEVELOPMENT_ONLY');
  }
}
