import { execFile } from "node:child_process";
import { existsSync } from "node:fs";
import { mkdir,mkdtemp,readFile,rm,utimes,writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { createServer } from "node:net";
import { promisify } from "node:util";
import sharp from "sharp";
import { describe,expect,it } from "vitest";
import { attachmentScannerReadiness,scanAttachmentBytes } from "../../../src/server/modules/files/public/attachments/scanner";
import { detectedAttachmentMediaType,sanitizedAttachmentPreview } from "../../../src/server/modules/files/public/attachments/preview";
import { ATTACHMENT_UPLOAD_MAX_BYTES,readBoundedUploadBody } from "../../../src/server/modules/files/public/attachments/upload";
import { validMessagingSignature } from "../../../src/server/modules/collaboration/public/messaging-files";

const execute=promisify(execFile);
const repository=process.cwd();
const scannerSocket=process.env.ERPX_CLAMD_SOCKET??join(repository,".scratch/dev/clamav/clamd.sock");
const scannerDatabase=process.env.ERPX_CLAMAV_DATABASE_DIR??join(repository,".scratch/dev/clamav/db");
const heifEncoder=process.env.ERPX_HEIF_ENC??"/opt/homebrew/opt/libheif/bin/heif-enc";
const heifConverter=process.env.ERPX_HEIF_CONVERT??"/opt/homebrew/opt/libheif/bin/heif-convert";

(existsSync(scannerSocket)&&["daily.cvd","daily.cld"].some(name=>existsSync(join(scannerDatabase,name)))?describe:describe.skip)("real attachment scanner",()=>{
  const environment={...process.env,ERPX_ATTACHMENTS_V2:"true",ERPX_CLAMD_SOCKET:scannerSocket,ERPX_CLAMAV_DATABASE_DIR:scannerDatabase};

  it("reports current ClamAV readiness and distinguishes clean bytes from EICAR",async()=>{
    const readiness=await attachmentScannerReadiness(environment);
    expect(readiness).toMatchObject({enabled:true,ready:true,error:null});
    expect(readiness.engine).toMatch(/ClamAV/i);
    expect(Number.isSafeInteger(readiness.definitionUpdatedAt)).toBe(true);
    await expect(scanAttachmentBytes(new TextEncoder().encode("clean ERPX attachment"),environment)).resolves.toMatchObject({clean:true,signature:null});
    const eicar=["X5O!P%@AP[4\\PZX54(P^)","7CC)7}$EICAR-STANDARD-","ANTIVIRUS-TEST-FILE!$H+H*"] .join("");
    await expect(scanAttachmentBytes(new TextEncoder().encode(eicar),environment)).resolves.toMatchObject({clean:false,signature:expect.stringMatching(/Eicar/i)});
  });

  it("fails closed when clamd is unavailable or its signatures are stale",async()=>{
    await expect(attachmentScannerReadiness({...environment,ERPX_CLAMD_SOCKET:join(repository,".scratch/dev/clamav/missing.sock")}))
      .resolves.toMatchObject({enabled:true,ready:false});
    const directory=await mkdtemp(join(tmpdir(),"erpx-stale-clamav-"));
    try{
      const definition=join(directory,"daily.cvd"),old=new Date(Date.now()-2*24*60*60_000);
      await writeFile(definition,"stale test definition");await utimes(definition,old,old);
      await expect(attachmentScannerReadiness({...environment,ERPX_CLAMAV_DATABASE_DIR:directory,ERPX_CLAMAV_MAX_DEFINITION_AGE_MS:"60000"}))
        .resolves.toMatchObject({enabled:true,ready:false,error:"CLAMAV_DEFINITIONS_STALE"});
    }finally{await rm(directory,{recursive:true,force:true});}
  });
});

(existsSync(heifEncoder)&&existsSync(heifConverter)?describe:describe.skip)("HEIC preview sanitization",()=>{
  it("decodes a real rotated non-square HEIC and strips source metadata",async()=>{
    const directory=await mkdtemp(join(tmpdir(),"erpx-heic-fixture-"));
    const prior=process.env.ERPX_HEIF_CONVERT;
    try{
      const png=join(directory,"source.png"),heic=join(directory,"fixture.heic");
      await sharp({create:{width:80,height:40,channels:3,background:{r:27,g:115,b:212}}})
        .withMetadata({orientation:1,exif:{IFD0:{ImageDescription:"private metadata"}}})
        .png().toFile(png);
      await execute(heifEncoder,["--rotate-cw","90","-q","70","-o",heic,png],{timeout:20_000,maxBuffer:1024*1024});
      const source=new Uint8Array(await readFile(heic));
      expect(detectedAttachmentMediaType(source,"application/octet-stream")).toBe("image/heic");
      expect(validMessagingSignature("image/heic",source)).toBe(true);
      process.env.ERPX_HEIF_CONVERT=heifConverter;
      const preview=await sanitizedAttachmentPreview(source,"image/heic");
      expect(preview?.mediaType).toBe("image/webp");
      const metadata=await sharp(preview!.bytes).metadata();
      expect(metadata.width).toBe(40);
      expect(metadata.height).toBe(80);
      expect(metadata.space).toBe("srgb");
      expect(metadata.exif).toBeUndefined();
      expect(metadata.iptc).toBeUndefined();
      expect(metadata.xmp).toBeUndefined();
    }finally{
      if(prior===undefined)delete process.env.ERPX_HEIF_CONVERT;else process.env.ERPX_HEIF_CONVERT=prior;
      await rm(directory,{recursive:true,force:true});
    }
  });
});

describe("bounded upload reader",()=>{
  it("cancels an oversized stream without calling Request.arrayBuffer",async()=>{
    let cancelReason:unknown=null,emitted=0;
    const body=new ReadableStream<Uint8Array>({
      pull(controller){
        emitted+=1;
        controller.enqueue(new Uint8Array(emitted===1?ATTACHMENT_UPLOAD_MAX_BYTES:1));
      },
      cancel(reason){cancelReason=reason;},
    });
    const request=new Request("http://erpx.test/upload",{method:"PUT",headers:{"content-length":String(ATTACHMENT_UPLOAD_MAX_BYTES)},body,duplex:"half"} as RequestInit&{duplex:"half"});
    Object.defineProperty(request,"arrayBuffer",{value:()=>{throw new Error("arrayBuffer must not be used");}});
    await expect(readBoundedUploadBody(request,ATTACHMENT_UPLOAD_MAX_BYTES)).rejects.toMatchObject({status:422,code:"UPLOAD_SIZE"});
    expect(emitted).toBe(2);
    expect(cancelReason).toBe("UPLOAD_SIZE");
  });
});

describe("fail-closed scanner protocol",()=>{
  it("times out a clamd scan that accepts INSTREAM but never returns a result",async()=>{
    const directory=await mkdtemp(join(tmpdir(),"erpx-clamd-timeout-")),socket=join(directory,"clamd.sock"),database=join(directory,"db");
    await mkdir(database);
    await writeFile(join(database,"daily.cvd"),"current test definition");
    const server=createServer(connection=>connection.on("data",chunk=>{
      const command=chunk.toString("utf8");
      if(command.startsWith("zPING"))connection.write("PONG\0");
      else if(command.startsWith("zVERSION"))connection.write("ClamAV test/1.0\0");
      // INSTREAM deliberately remains open and silent.
    }));
    try{
      await new Promise<void>((resolve,reject)=>{server.once("error",reject);server.listen(socket,resolve);});
      const environment={...process.env,ERPX_ATTACHMENTS_V2:"true",ERPX_CLAMD_SOCKET:socket,ERPX_CLAMAV_DATABASE_DIR:database,ERPX_CLAMD_SCAN_TIMEOUT_MS:"50"};
      await expect(scanAttachmentBytes(new TextEncoder().encode("bounded timeout fixture"),environment)).rejects.toThrow("CLAMD_SCAN_TIMEOUT");
    }finally{
      await new Promise<void>(resolve=>server.close(()=>resolve()));
      await rm(directory,{recursive:true,force:true});
    }
  });
});

describe("attachment media detection",()=>{
  it("rejects malformed files and conflicting appended containers",async()=>{
    const append=(left:Uint8Array,right:Uint8Array)=>{const value=new Uint8Array(left.byteLength+right.byteLength);value.set(left);value.set(right,left.byteLength);return value;};
    const zipTail=new Uint8Array([0x50,0x4b,0x05,0x06,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]);
    const pdf=new TextEncoder().encode("%PDF-1.7\n1 0 obj<<>>endobj\nstartxref\n0\n%%EOF\n");
    expect(detectedAttachmentMediaType(pdf,"application/pdf")).toBe("application/pdf");
    expect(detectedAttachmentMediaType(append(pdf,zipTail),"application/pdf")).toBeNull();
    const pngBytes=new Uint8Array(await sharp({create:{width:2,height:2,channels:3,background:"red"}}).png().toBuffer());
    const jpegBytes=new Uint8Array(await sharp({create:{width:2,height:2,channels:3,background:"red"}}).jpeg().toBuffer());
    const webpBytes=new Uint8Array(await sharp({create:{width:2,height:2,channels:3,background:"red"}}).webp().toBuffer());
    expect(detectedAttachmentMediaType(pngBytes,"image/png")).toBe("image/png");
    expect(detectedAttachmentMediaType(jpegBytes,"image/jpeg")).toBe("image/jpeg");
    expect(detectedAttachmentMediaType(webpBytes,"image/webp")).toBe("image/webp");
    expect(detectedAttachmentMediaType(append(pngBytes,zipTail),"image/png")).toBeNull();
    expect(detectedAttachmentMediaType(append(jpegBytes,zipTail),"image/jpeg")).toBeNull();
    expect(detectedAttachmentMediaType(append(webpBytes,zipTail),"image/webp")).toBeNull();
    expect(detectedAttachmentMediaType(pngBytes.slice(0,-4),"image/png")).toBeNull();
  });

  (existsSync(heifEncoder)?it:it.skip)("rejects truncated and appended HEIC containers",async()=>{
    const directory=await mkdtemp(join(tmpdir(),"erpx-heic-detection-"));
    try{
      const png=join(directory,"source.png"),heic=join(directory,"fixture.heic");
      await sharp({create:{width:8,height:6,channels:3,background:"blue"}}).png().toFile(png);
      await execute(heifEncoder,["-q","70","-o",heic,png],{timeout:20_000,maxBuffer:1024*1024});
      const source=new Uint8Array(await readFile(heic)),zipTail=new Uint8Array([0x50,0x4b,0x03,0x04,0,0,0,0]);
      const appended=new Uint8Array(source.byteLength+zipTail.byteLength);appended.set(source);appended.set(zipTail,source.byteLength);
      expect(detectedAttachmentMediaType(source,"image/heic")).toBe("image/heic");
      expect(detectedAttachmentMediaType(source.slice(0,-1),"image/heic")).toBeNull();
      expect(detectedAttachmentMediaType(appended,"image/heic")).toBeNull();
    }finally{await rm(directory,{recursive:true,force:true});}
  });

  it("rejects an image whose declared dimensions exceed the decode pixel limit",async()=>{
    const source=new Uint8Array(await sharp({create:{width:2,height:2,channels:3,background:"red"}}).png().toBuffer());
    const crc32=(value:Uint8Array)=>{
      let crc=0xffffffff;
      for(const byte of value){crc^=byte;for(let bit=0;bit<8;bit++)crc=(crc>>>1)^((crc&1)?0xedb88320:0);}
      return (crc^0xffffffff)>>>0;
    };
    const bomb=source.slice(),view=new DataView(bomb.buffer,bomb.byteOffset,bomb.byteLength);
    view.setUint32(16,100_000,false);view.setUint32(20,100_000,false);
    view.setUint32(29,crc32(bomb.subarray(12,29)),false);
    expect(detectedAttachmentMediaType(bomb,"image/png")).toBe("image/png");
    await expect(sanitizedAttachmentPreview(bomb,"image/png")).rejects.toThrow();
  });
});
