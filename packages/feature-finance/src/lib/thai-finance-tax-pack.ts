/** Primary-source interpretation and captured byte digests: docs/research/thai-finance-tax-rules-2026-10-03.md. */
export const thaiFinanceTaxPack = Object.freeze({
  adapter: "thai-domestic-nonpayroll-wht-v1", version: "TH-WHT-2026-10-03-r1",
  effectiveFrom: "2026-01-01", effectiveTo: "2026-12-31", currency: "THB", filingMode: "ordinary", certified: false,
  sources: [
    { kind: "ordinary_rates", url: "https://www.rd.go.th/3479.html", sha256: "fc34cd223117287b961d06f48d0115748b8dd7133b0fbe1d12f5517083ef2d46" },
    { kind: "certificate_timing", url: "https://www.rd.go.th/5937.html", sha256: "7c4b70b52f97b39e371284cdfdcab9f75b56a44b27cadd549bdf9934221e882a" },
    { kind: "certificate_form", url: "https://www.rd.go.th/3171.html", sha256: "7c0d649d21615722f9a749183ce124bfd21a633c36b701a97c6921d7935e8731" },
    { kind: "tax_invoice", url: "https://www.rd.go.th/5208.html", sha256: "2ea37b8583d096e90dea6d56290bd24e1ec8eb4dbb40f8f972a73f06cff2ad56" },
    { kind: "vat_rate_through_2026",url:"https://www.rd.go.th/fileadmin/user_upload/kormor/newlaw/dc799.pdf",sha256:"796b9aff527081720587dcf5b378833eb09bac731c3cc941fea382ce18302ba3" },
    { kind: "vat_rate_extension_2027",url:"https://www.rd.go.th/fileadmin/user_upload/kormor/newlaw/dc807.pdf",sha256:"a4e0c55a2324397af4079317d2a5138fa075fcd87eb9348771742ed7ad6dab70" },
  ],
  ordinaryVat:{effectiveFrom:"2025-10-01",effectiveTo:"2027-09-30",rateBps:700},
  rates: {
    service: { label: "ค่าบริการอื่น", section: "40(8)", rateBps: 300, orderClause: "12/1" },
    hire_of_work: { label: "ค่าจ้างทำของ", section: "40(7)/(8)", rateBps: 300, orderClause: "8" },
    professional: { label: "ค่าวิชาชีพอิสระ", section: "40(6)", rateBps: 300, orderClause: "7" },
    rent: { label: "ค่าเช่าทรัพย์สิน", section: "40(5)", rateBps: 500, orderClause: "6" },
    advertising: { label: "ค่าโฆษณา", section: "40(8)", rateBps: 200, orderClause: "10" },
    transport: { label: "ค่าขนส่งที่มิใช่ค่าโดยสารสาธารณะ", section: "40(8)", rateBps: 100, orderClause: "12/4" },
    non_life_insurance: { label: "เบี้ยประกันวินาศภัย", section: "40(8)", rateBps: 100, orderClause: "12/3" },
    goods: { label: "ซื้อสินค้าทั่วไปที่ไม่เข้ารายการเฉพาะตามคำสั่ง", section: "40(8)", rateBps: 0, orderClause: "ordinary_goods_outside_list" },
    hotel_restaurant: { label: "ค่าบริการโรงแรมหรือภัตตาคาร", section: "40(8)", rateBps: 0, orderClause: "12/1_exclusion" },
    public_transport: { label: "ค่าโดยสารสำหรับการขนส่งสาธารณะ", section: "40(8)", rateBps: 0, orderClause: "12/4_exclusion" },
  },
});
export type ThaiWithholdingCategory = keyof typeof thaiFinanceTaxPack.rates;

export function thaiWithholdingAmount(incomeMinor: number, category: ThaiWithholdingCategory, contractTotalMinor: number) {
  if (!Number.isSafeInteger(incomeMinor) || incomeMinor < 0 || !Number.isSafeInteger(contractTotalMinor) || contractTotalMinor < incomeMinor) throw new Error("The contract must cover the taxable income paid.");
  const rule = thaiFinanceTaxPack.rates[category];
  const rateBps = contractTotalMinor < 100_000 ? 0 : rule.rateBps;
  return { rateBps, taxMinor: Number((BigInt(incomeMinor) * BigInt(rateBps) + BigInt(5000)) / BigInt(10_000)), rule };
}

/** Thai identity-number check digit; evidence and legal-party type are reviewed separately. */
export function validThaiTaxId(value: string) {
  if (!/^\d{13}$/.test(value) || /^0{13}$/.test(value)) return false;
  const sum = [...value.slice(0, 12)].reduce((total, digit, index) => total + Number(digit) * (13 - index), 0);
  return (11 - (sum % 11)) % 10 === Number(value[12]);
}

function thaiInteger(value: bigint): string {
  if (value === BigInt(0)) return "ศูนย์";
  if (value >= BigInt(1_000_000)) return `${thaiInteger(value / BigInt(1_000_000))}ล้าน${value % BigInt(1_000_000) ? thaiInteger(value % BigInt(1_000_000)) : ""}`;
  const digits = ["", "หนึ่ง", "สอง", "สาม", "สี่", "ห้า", "หก", "เจ็ด", "แปด", "เก้า"], places = ["", "สิบ", "ร้อย", "พัน", "หมื่น", "แสน"];
  const text = String(value); let result = "";
  for (let index = 0; index < text.length; index++) {
    const digit = Number(text[index]), place = text.length - index - 1;
    if (!digit) continue;
    if (place === 1) result += digit === 1 ? "สิบ" : digit === 2 ? "ยี่สิบ" : `${digits[digit]}สิบ`;
    else if (place === 0) result += digit === 1 && text.length > 1 ? "เอ็ด" : digits[digit];
    else result += `${digits[digit]}${places[place]}`;
  }
  return result;
}
export function thaiBahtWords(minor: number) {
  if (!Number.isSafeInteger(minor) || minor < 0) throw new Error("Use a nonnegative exact Thai-baht amount.");
  const value = BigInt(minor), satang = value % BigInt(100);
  return `${thaiInteger(value / BigInt(100))}บาท${satang ? `${thaiInteger(satang)}สตางค์` : "ถ้วน"}`;
}
