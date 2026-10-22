export function detectedAttachmentMediaType(bytes: Uint8Array, declared: string) {
  const ascii = (from: number, length: number) => new TextDecoder("ascii", { fatal: false }).decode(bytes.slice(from, from + length));
  const paddingOnly=(from:number)=>{for(let index=from;index<bytes.byteLength;index++)if(![0,9,10,12,13,32].includes(bytes[index]!))return false;return true;};
  const detectedHeifBrand=()=>{
    if(bytes.byteLength<16)return null;
    const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength),brands=new Set<string>(),boxTypes=new Set<string>();
    let offset=0,first=true;
    while(offset<bytes.byteLength){
      if(bytes.byteLength-offset<8)return null;
      const size32=view.getUint32(offset,false),type=ascii(offset+4,4);
      let headerBytes=8,size=size32;
      if(size32===1){
        if(bytes.byteLength-offset<16)return null;
        const extended=view.getBigUint64(offset+8,false);
        if(extended>BigInt(Number.MAX_SAFE_INTEGER))return null;
        size=Number(extended);headerBytes=16;
      }else if(size32===0)size=bytes.byteLength-offset;
      if(size<headerBytes||offset+size>bytes.byteLength)return null;
      if(first){
        if(type!=="ftyp"||size<headerBytes+8)return null;
        brands.add(ascii(offset+headerBytes,4));
        for(let brandOffset=offset+headerBytes+8;brandOffset+4<=offset+size;brandOffset+=4)brands.add(ascii(brandOffset,4));
        first=false;
      }
      boxTypes.add(type);
      offset+=size;
      if(size32===0&&offset!==bytes.byteLength)return null;
    }
    if(offset!==bytes.byteLength||!boxTypes.has("meta"))return null;
    const heic=["heic","heix","hevc","hevx","heim","heis"].some(brand=>brands.has(brand));
    if(heic)return "image/heic" as const;
    return ["mif1","msf1"].some(brand=>brands.has(brand))?"image/heif" as const:null;
  };
  if (bytes.byteLength >= 5 && ascii(0, 5) === "%PDF-") {
    const tailStart=Math.max(0,bytes.byteLength-4096),tail=ascii(tailStart,bytes.byteLength-tailStart),eof=tail.lastIndexOf("%%EOF");
    return eof>=0&&paddingOnly(tailStart+eof+5)?"application/pdf":null;
  }
  if (bytes.byteLength >= 20 && bytes[0] === 0x89 && ascii(1, 3) === "PNG") {
    const suffix=[0,0,0,0,0x49,0x45,0x4e,0x44,0xae,0x42,0x60,0x82];
    return suffix.every((value,index)=>bytes[bytes.byteLength-suffix.length+index]===value)?"image/png":null;
  }
  if (bytes.byteLength >= 5 && bytes[0] === 0xff && bytes[1] === 0xd8 && bytes[2] === 0xff) {
    let end=-1;for(let index=bytes.byteLength-2;index>=2;index--)if(bytes[index]===0xff&&bytes[index+1]===0xd9){end=index+2;break;}
    return end>0&&paddingOnly(end)?"image/jpeg":null;
  }
  if (bytes.byteLength >= 12 && ascii(0, 4) === "RIFF" && ascii(8, 4) === "WEBP") {
    const declaredBytes=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength).getUint32(4,true)+8;
    return declaredBytes===bytes.byteLength?"image/webp":null;
  }
  const heif=detectedHeifBrand();if(heif)return heif;
  if(declared==="application/vnd.openxmlformats-officedocument.wordprocessingml.document"&&bytes.byteLength>=22&&bytes[0]===0x50&&bytes[1]===0x4b){
    const view=new DataView(bytes.buffer,bytes.byteOffset,bytes.byteLength),minimum=Math.max(0,bytes.byteLength-65_557);let eocd=-1;
    for(let offset=bytes.byteLength-22;offset>=minimum;offset--)if(view.getUint32(offset,true)===0x06054b50){eocd=offset;break;}
    if(eocd>=0){const centralSize=view.getUint32(eocd+12,true),centralOffset=view.getUint32(eocd+16,true),end=centralOffset+centralSize,names=new Set<string>();let offset=centralOffset;
      while(offset+46<=end&&end<=bytes.byteLength&&view.getUint32(offset,true)===0x02014b50){const nameLength=view.getUint16(offset+28,true),extraLength=view.getUint16(offset+30,true),commentLength=view.getUint16(offset+32,true),nameEnd=offset+46+nameLength;if(nameEnd>end)break;names.add(new TextDecoder().decode(bytes.subarray(offset+46,nameEnd)));offset=nameEnd+extraLength+commentLength;}
      if(names.has("[Content_Types].xml")&&names.has("word/document.xml"))return declared;
    }
    return null;
  }
  if (declared === "text/csv") {
    if (bytes.includes(0)) return null;
    try { new TextDecoder("utf-8", { fatal: true }).decode(bytes); return "text/csv"; } catch { return null; }
  }
  return null;
}
