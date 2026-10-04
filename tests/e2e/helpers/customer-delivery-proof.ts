/** A valid one-page fictional receiving note for actual scan and PDF preview. */
export function customerDeliveryProofPdf(): Buffer {
  const stream = "BT /F1 16 Tf 36 740 Td (FICTIONAL ERPX DELIVERY RECEIPT) Tj 0 -30 Td /F1 12 Tf (Synthetic customer confirms one simulation item received.) Tj 0 -24 Td (Engineering fixture only - no provider or business certification.) Tj ET\n";
  const objects = [
    "<< /Type /Catalog /Pages 2 0 R >>",
    "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>",
    "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>",
    `<< /Length ${Buffer.byteLength(stream)} >>\nstream\n${stream}endstream`,
  ];
  const chunks = [Buffer.from("%PDF-1.4\n")], offsets: number[] = [];
  let length = chunks[0].length;
  for (const [index, object] of objects.entries()) {
    offsets.push(length);
    const chunk = Buffer.from(`${index + 1} 0 obj\n${object}\nendobj\n`);
    chunks.push(chunk);
    length += chunk.length;
  }
  chunks.push(Buffer.from(`xref\n0 6\n0000000000 65535 f \n${offsets.map(offset => `${String(offset).padStart(10, "0")} 00000 n \n`).join("")}trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${length}\n%%EOF\n`));
  return Buffer.concat(chunks);
}
