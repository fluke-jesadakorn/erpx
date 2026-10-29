/** Package observations supplement, and never replace, the real decoder probe. */
export function ciHeifDecoderInventory(stdout:string):{name:string;version:string}[]{
  const required=['libheif1','libheif-examples','libheif-plugin-libde265','libde265-0'];
  if(stdout.length>4096)throw new Error('CI_HEVC_DECODER_INVENTORY_INVALID');
  const rows=stdout.trim().split('\n').map(line=>{
    const fields=line.split('\t');
    if(fields.length!==2||!required.includes(fields[0]!)||!fields[1]||fields[1].length>120||!/^[A-Za-z0-9.+:~_-]+$/.test(fields[1]))
      throw new Error('CI_HEVC_DECODER_INVENTORY_INVALID');
    return {name:fields[0]!,version:fields[1]};
  });
  if(rows.length!==required.length||required.some(name=>rows.filter(row=>row.name===name).length!==1))
    throw new Error('CI_HEVC_DECODER_PACKAGES_REQUIRED');
  return rows.sort((a,b)=>a.name.localeCompare(b.name));
}
