type ProductBrandProps = {
  variant?: "symbol" | "wordmark" | "lockup";
  inverse?: boolean;
  className?: string;
};

function Symbol({ inverse = false }: { inverse?: boolean }) {
  const color = inverse ? "#FFFFFF" : "var(--brand, #2F69F5)";
  return (
    <svg viewBox="0 0 128 128" aria-hidden="true">
      <g fill={color}>
        <path d="M18 28 28 18H112L90 40H18Z" />
        <path d="M18 63 28 53H90L68 75H18Z" />
        <path d="M18 98 28 88H112L90 110H18Z" />
      </g>
    </svg>
  );
}

function Wordmark({ inverse = false }: { inverse?: boolean }) {
  const color = inverse ? "#FFFFFF" : "var(--brand, #2F69F5)";
  return (
    <svg viewBox="0 0 536 100" aria-hidden="true">
      <g fill={color}>
        <g transform="scale(1.08695652173913) translate(-18 -18)">
          <path d="M18 28 28 18H112L90 40H18Z" />
          <path d="M18 63 28 53H90L68 75H18Z" />
          <path d="M18 98 28 88H112L90 110H18Z" />
        </g>
        <g fillRule="evenodd">
          <path transform="translate(132)" d="M0 0H82L108 26V40L86 62H72L112 100H85L45 62H18V100H0ZM18 18V44H78L90 32 76 18Z" />
          <path transform="translate(274)" d="M0 0H82L108 26V40L86 62H18V100H0ZM18 18V44H78L90 32 76 18Z" />
          <path transform="translate(414)" opacity="0.48" d="M0 0H26L56 34 86 0H112L69 50 112 100H86L56 66 26 100H0L43 50Z" />
        </g>
      </g>
    </svg>
  );
}

export function ProductBrand({ variant = "lockup", inverse = false, className }: ProductBrandProps) {
  return (
    <span className={className} aria-label="ERPX">
      {variant === "symbol" ? <Symbol inverse={inverse} /> : <Wordmark inverse={inverse} />}
    </span>
  );
}
