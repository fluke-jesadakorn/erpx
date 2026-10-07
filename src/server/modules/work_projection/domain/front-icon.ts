function escapeXml(value: string) {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll('"', "&quot;").replaceAll("'", "&apos;");
}

export function storefrontAppIconSvg(args: {
  shortName: string;
  accentColor: string;
  app: "pos" | "pms";
  size: number;
  logo?: string;
}) {
  const { shortName, app, size, logo } = args;
  const color = /^#[0-9a-fA-F]{6}$/.test(args.accentColor) ? args.accentColor : "#3659c9";
  const initial = escapeXml(Array.from(shortName.trim())[0]?.toLocaleUpperCase() ?? "•");
  const mark = logo?.length
    ? '<image x="' + Math.round(size * 0.16) + '" y="' + Math.round(size * 0.12) + '" width="' + Math.round(size * 0.62) + '" height="' + Math.round(size * 0.60) + '" preserveAspectRatio="xMidYMid meet" href="data:image/webp;base64,' + logo + '"/>'
    : '<text x="50%" y="46%" text-anchor="middle" dominant-baseline="middle" font-family="Arial,sans-serif" font-size="' + Math.round(size * 0.48) + '" font-weight="700" fill="#fff">' + initial + "</text>";
  const badge = '<rect x="' + Math.round(size * 0.52) + '" y="' + Math.round(size * 0.72) + '" width="' + Math.round(size * 0.43) + '" height="' + Math.round(size * 0.20) + '" rx="' + Math.round(size * 0.07) + '" fill="#fff"/><text x="' + Math.round(size * 0.735) + '" y="' + Math.round(size * 0.82) + '" text-anchor="middle" dominant-baseline="middle" font-family="Arial,sans-serif" font-size="' + Math.round(size * 0.085) + '" font-weight="700" letter-spacing="1" fill="' + color + '">' + (app === "pos" ? "POS" : "PMS") + "</text>";
  const svg = '<svg xmlns="http://www.w3.org/2000/svg" width="' + size + '" height="' + size + '" viewBox="0 0 ' + size + " " + size + '"><rect width="100%" height="100%" fill="' + color + '"/>' + mark + badge + "</svg>";
  return svg;
}
