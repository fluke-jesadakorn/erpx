declare module '*/ocr_lib/pdf-pipeline.js' {
  export function extractText(pdfBuffer: Buffer): Promise<Record<string, unknown>>;
  export function extractViaOllamaVision(fileBuffer: Buffer, mime: string, fname: string): Promise<Record<string, unknown>>;
}