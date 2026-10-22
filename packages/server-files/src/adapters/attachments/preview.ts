import "server-only";

import { execFile } from "node:child_process";
import { constants } from "node:fs";
import { access, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { isAbsolute, join } from "node:path";
import { promisify } from "node:util";

const execute = promisify(execFile);

function executable(name:"heif"|"pdf",environment:NodeJS.ProcessEnv=process.env){
  const value=name==="heif"?(environment.ERPX_HEIF_CONVERT??"/opt/homebrew/opt/libheif/bin/heif-convert"):(environment.ERPX_PDFTOPPM_BIN??"/opt/homebrew/bin/pdftoppm");
  if(!isAbsolute(value))throw new Error(name==="heif"?"ERPX_HEIF_CONVERT_ABSOLUTE_REQUIRED":"ERPX_PDFTOPPM_BIN_ABSOLUTE_REQUIRED");return value;
}

export async function attachmentPreviewReadiness(environment:NodeJS.ProcessEnv=process.env){
  try{const heifConvert=executable("heif",environment),pdftoppm=executable("pdf",environment);await Promise.all([access(heifConvert,constants.X_OK),access(pdftoppm,constants.X_OK)]);return {ready:true,heifConvert,pdftoppm,error:null};}
  catch(error){return {ready:false,heifConvert:null,pdftoppm:null,error:error instanceof Error?error.message:"ATTACHMENT_PREVIEW_TOOLS_UNAVAILABLE"};}
}

export async function sanitizedAttachmentPreview(bytes: Uint8Array, mediaType: string): Promise<{ bytes: Uint8Array; mediaType: "image/webp" | "image/jpeg" } | null> {
  if(mediaType==="image/heic"||mediaType==="image/heif"){
    const sharp=(await import("sharp")).default,metadata=await sharp(bytes,{failOn:"warning",limitInputPixels:40_000_000,sequentialRead:true}).metadata();
    const width=metadata.width??0,height=metadata.height??0;
    if(width<1||height<1||width*height>40_000_000)throw new Error("HEIF_PIXEL_LIMIT");
    const directory=await mkdtemp(join(tmpdir(),"erpx-heif-preview-"));
    try{
      const source=join(directory,mediaType==="image/heic"?"source.heic":"source.heif"),decoded=join(directory,"decoded.png");await writeFile(source,bytes,{mode:0o600});await writeFile(decoded,new Uint8Array(),{mode:0o600});
      await execute(executable("heif"),[source,decoded],{timeout:20_000,maxBuffer:1024*1024});
      const decodedStat=await stat(decoded);if(decodedStat.size<1||decodedStat.size>100*1024*1024)throw new Error("HEIF_DECODE_OUTPUT_INVALID");
      const output=await sharp(decoded,{failOn:"warning",limitInputPixels:40_000_000,sequentialRead:true}).rotate().resize({width:2400,height:2400,fit:"inside",withoutEnlargement:true}).toColourspace("srgb").webp({quality:82,effort:4}).toBuffer();
      return {bytes:new Uint8Array(output),mediaType:"image/webp"};
    }finally{await rm(directory,{recursive:true,force:true});}
  }
  if (mediaType.startsWith("image/")) {
    const sharp = (await import("sharp")).default;
    const output = await sharp(bytes, { failOn: "warning", limitInputPixels: 40_000_000, sequentialRead: true })
      .rotate()
      .resize({ width: 2400, height: 2400, fit: "inside", withoutEnlargement: true })
      .toColourspace("srgb")
      .webp({ quality: 82, effort: 4 })
      .toBuffer();
    return { bytes: new Uint8Array(output), mediaType: "image/webp" };
  }
  if (mediaType !== "application/pdf") return null;
  const directory = await mkdtemp(join(tmpdir(), "erpx-attachment-preview-"));
  try {
    const source = join(directory, "source.pdf"), target = join(directory, "preview");
    await writeFile(source, bytes, { mode: 0o600 });
    await execute(executable("pdf"), ["-f", "1", "-l", "1", "-singlefile", "-scale-to", "2400", "-jpeg", source, target], { timeout: 20_000, maxBuffer: 1024 * 1024 });
    return { bytes: new Uint8Array(await readFile(`${target}.jpg`)), mediaType: "image/jpeg" };
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

export {detectedAttachmentMediaType} from "@erpx/server-files/domain/attachments/media-signature";
