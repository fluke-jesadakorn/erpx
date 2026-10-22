/** Only the same Holding's canonical Front routes can become return links. */
export function frontWorkReturn(value: string | null, holdingSlug: string, companySlug?: string) {
  if (!value || !value.startsWith("/front/") || value.includes("\\") || value.length > 2000) return null;
  const url = new URL(value,"https://work.invalid");
  const segments = url.pathname.split("/").filter(Boolean);
  if (segments[0] !== "front" || segments[1] !== encodeURIComponent(holdingSlug) || companySlug && segments[2] !== encodeURIComponent(companySlug) || !["pos","pms"].includes(segments[3]) || segments.length<5 ||
    segments.some(segment=>{try{return /[/\\]|^\.{1,2}$/.test(decodeURIComponent(segment));}catch{return true;}})) return null;
  return `${url.pathname}${url.search}${url.hash}`;
}
