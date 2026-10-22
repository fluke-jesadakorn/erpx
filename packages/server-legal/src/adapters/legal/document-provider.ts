import { runtimeResourcePath } from "@erpx/server-runtime/public/runtime/resources";
import "server-only";
import {readFile,mkdtemp,writeFile,rm,readdir} from "node:fs/promises";
import {tmpdir} from "node:os";
import {join,resolve} from "node:path";
import {execFile} from "node:child_process";
import {promisify} from "node:util";
import {Document,Paragraph,TextRun,Packer} from "docx";
import {PDFDocument} from "pdf-lib";
import fontkit from "@pdf-lib/fontkit";
import type {DocumentProvider} from "@erpx/server-legal/application/documents";
import {validateSignedPdf} from "@erpx/server-legal/adapters/legal/pdf-validation";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
const exec=promisify(execFile);
export function legalDocumentProvider(db:SqlDatabase,companyId:string,error:(status:number,code:string,message:string)=>Error):DocumentProvider{return {
 docx:async body=>{
    return await Packer.toBuffer(
      new Document({
        sections: [
          {
            children: body.split("\n").map(
              (t) =>
                new Paragraph({
                  children: [
                    new TextRun({
                      text: t,
                      font: "Noto Sans Thai",
                      size: 24,
                    }),
                  ],
                  spacing: { after: 160 },
                }),
            ),
          },
        ],
      }),
    );

},pdf:async body=>{
    const pdf = await PDFDocument.create();
    pdf.registerFontkit(fontkit);
    const font = await pdf.embedFont(
      await readFile(runtimeResourcePath("public/fonts/NotoSansThai.ttf")),
      { subset: true },
    );
    let page = pdf.addPage([595, 842]),
      y = 790;
    for (const paragraph of body.split("\n")) {
      let line = "";
      for (const char of Array.from(paragraph)) {
        if (line && font.widthOfTextAtSize(line + char, 11) > 491) {
          page.drawText(line, { x: 52, y, size: 11, font });
          y -= 19;
          line = "";
          if (y < 55) {
            page = pdf.addPage([595, 842]);
            y = 790;
          }
        }
        line += char;
      }
      if (line) page.drawText(line, { x: 52, y, size: 11, font });
      y -= 22;
      if (y < 55) {
        page = pdf.addPage([595, 842]);
        y = 790;
      }
    }
    return pdf.save();

},prepare:async()=>{const directory=await mkdtemp(join(tmpdir(),"erpx-legal-")),path=join(directory,"input");return {write:bytes=>writeFile(path,bytes,{mode:0o600}),verify:()=>validateSignedPdf(db,companyId,path,directory),extract:async d=>{
      let text = d.body;
      let pageTexts:{page:number;text:string}[]=[];
      const pageMapping=d.media_type!=="application/vnd.openxmlformats-officedocument.wordprocessingml.document";
      if (d.object_key) {
        if (d.media_type === "application/pdf"){
          text = (
            await exec(
              process.env.ERPX_PDFTOTEXT_BIN ?? "/opt/homebrew/bin/pdftotext",
              ["-layout", path, "-"],
              { timeout: 30000, maxBuffer: 2 * 1024 * 1024 },
            )
          ).stdout;
          const segments=text.split("\f");
          if(!segments[segments.length-1]?.trim())segments.pop();
          pageTexts=segments.map((text,i)=>({page:i+1,text:text.slice(0,200000)}));
        }
        else if (d.media_type.startsWith("image/"))
          text = (
            await exec(
              process.env.ERPX_TESSERACT_BIN ?? "/opt/homebrew/bin/tesseract",
              [
                path,
                "stdout",
                "-l",
                process.env.ERPX_OCR_LANGUAGES ?? "eng+tha",
                "--tessdata-dir",
                process.env.ERPX_TESSDATA_DIR ??
                  resolve(".runtime/legal-ocr/tessdata"),
              ],
              { timeout: 45000, maxBuffer: 2 * 1024 * 1024 },
            )
          ).stdout;
        else if (
          d.media_type ===
          "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        )
          text = (
            await exec(
              process.execPath,
              [
                "--max-old-space-size=256",
                resolve("scripts/legal/extract_docx.mjs"),
                path,
              ],
              { timeout: 30000, maxBuffer: 1024 * 1024 },
            )
          ).stdout;
        else throw error(422,"LEGAL_INVALID","Unsupported text extraction format.");
        if (d.media_type === "application/pdf" && !text.trim()) {
          const info = (
            await exec(
              process.env.ERPX_PDFINFO_BIN ?? "/opt/homebrew/bin/pdfinfo",
              [path],
              { timeout: 15000, maxBuffer: 100000 },
            )
          ).stdout;
          const pages = Number(info.match(/^Pages:\s+(\d+)/m)?.[1]);
          if (!pages || pages > 25)
            throw error(422,"LEGAL_INVALID","Scanned PDFs are limited to 25 pages per document.");
          await exec(
            process.env.ERPX_PDFTOPPM_BIN ?? "/opt/homebrew/bin/pdftoppm",
            [
              "-png",
              "-scale-to",
              "1600",
              "-f",
              "1",
              "-l",
              String(pages),
              path,
              join(directory, "page"),
            ],
            { timeout: 90000, maxBuffer: 100000 },
          );
          const texts = [];
          for (const page of (await readdir(directory))
            .filter((n) => /^page-.*\.png$/.test(n))
            .sort())
            texts.push(
              (
                await exec(
                  process.env.ERPX_TESSERACT_BIN ??
                    "/opt/homebrew/bin/tesseract",
                  [
                    join(directory, page),
                    "stdout",
                    "-l",
                    process.env.ERPX_OCR_LANGUAGES ?? "eng+tha",
                    "--tessdata-dir",
                    process.env.ERPX_TESSDATA_DIR ??
                      resolve(".runtime/legal-ocr/tessdata"),
                  ],
                  { timeout: 20000, maxBuffer: 1000000 },
                )
              ).stdout,
            );
          text = texts.join("\n\n");
          pageTexts=texts.map((text,i)=>({page:i+1,text:text.slice(0,200000)}));
        }
      }

return {text,pages:pageTexts,pageMapping};},close:()=>rm(directory,{recursive:true,force:true})};}
};}
