const assetBase = "/vendor/pdfjs-6.4.299/";
export const maxDocumentPages = 20;
async function imageBlob(canvas: HTMLCanvasElement) {
  return new Promise<Blob>((resolve, reject) =>
    canvas.toBlob(
      (blob) =>
        blob && blob.size <= 2 * 1024 * 1024
          ? resolve(blob)
          : reject(
              Error("This page is too large to prepare. Continue manually."),
            ),
      "image/jpeg",
      0.85,
    ),
  );
}
/** Render one page at a time. Never execute PDF scripting or follow document links. */
export async function prepareDocumentPages(
  file: File,
  save: (page: number, total: number, image: Blob) => Promise<void>,
) {
  if (file.type === "application/pdf") {
    const pdfjs = await import("pdfjs-dist");
    pdfjs.GlobalWorkerOptions.workerSrc = `${assetBase}pdf.worker.min.mjs`;
    const task = pdfjs.getDocument({
      data: new Uint8Array(await file.arrayBuffer()),
      enableXfa: false,
      wasmUrl: `${assetBase}wasm/`,
      cMapUrl: `${assetBase}cmaps/`,
      cMapPacked: true,
      standardFontDataUrl: `${assetBase}standard_fonts/`,
    });
    task.onPassword = () => {
      void task.destroy();
    };
    try {
      const document = await task.promise;
      if (document.numPages > maxDocumentPages)
        throw Error(
          `AI reads up to ${maxDocumentPages} pages per document. Keep this original and complete the claim manually.`,
        );
      for (let pageNumber = 1; pageNumber <= document.numPages; pageNumber++) {
        const page = await document.getPage(pageNumber),
          size = page.getViewport({ scale: 1 }),
          viewport = page.getViewport({
            scale: Math.min(2, 1600 / Math.max(size.width, size.height)),
          }),
          canvas = globalThis.document.createElement("canvas");
        canvas.width = Math.ceil(viewport.width);
        canvas.height = Math.ceil(viewport.height);
        await page.render({ canvas, viewport }).promise;
        await save(pageNumber, document.numPages, await imageBlob(canvas));
        canvas.width = 0;
        canvas.height = 0;
        page.cleanup();
      }
    } finally {
      await task.destroy();
    }
  } else if (["image/jpeg", "image/png", "image/webp"].includes(file.type)) {
    const bitmap = await createImageBitmap(file);
    try {
      const scale = Math.min(1, 1600 / Math.max(bitmap.width, bitmap.height)),
        canvas = document.createElement("canvas");
      canvas.width = Math.ceil(bitmap.width * scale);
      canvas.height = Math.ceil(bitmap.height * scale);
      const context = canvas.getContext("2d");
      if (!context) throw Error("Image preparation is unavailable.");
      context.fillStyle = "#fff";
      context.fillRect(0, 0, canvas.width, canvas.height);
      context.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
      await save(1, 1, await imageBlob(canvas));
      canvas.width = 0;
      canvas.height = 0;
    } finally {
      bitmap.close();
    }
  }
}
