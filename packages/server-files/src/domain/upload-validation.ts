export function matchesUploadSignature(type: string, bytes: Uint8Array) {
  if (type === "application/pdf")
    return new TextDecoder().decode(bytes.slice(0, 5)) === "%PDF-";
  if (type === "image/png")
    return bytes[0] === 0x89 && bytes[1] === 0x50 && bytes[2] === 0x4e && bytes[3] === 0x47;
  if (type === "image/jpeg") return bytes[0] === 0xff && bytes[1] === 0xd8;
  if (type === "image/webp")
    return new TextDecoder().decode(bytes.slice(0, 4)) === "RIFF"
      && new TextDecoder().decode(bytes.slice(8, 12)) === "WEBP";
  if ((type === "image/heic" || type === "image/heif") && bytes.byteLength >= 12) {
    const text = new TextDecoder("ascii");
    if (text.decode(bytes.slice(4, 8)) !== "ftyp") return false;
    const brand = text.decode(bytes.slice(8, 12));
    return type === "image/heic"
      ? ["heic", "heix", "hevc", "hevx", "heim", "heis"].includes(brand)
      : ["mif1", "msf1"].includes(brand);
  }
  return type === "text/csv";
}
