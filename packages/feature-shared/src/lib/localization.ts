/** ERPX has one application language and one default operating currency. */
export const APP_LOCALE = "en-GB";
export const APP_CURRENCY = "THB";
export const APP_TIMEZONE = "Asia/Bangkok";

export function formatMoney(
  minor: number,
  currency: string = APP_CURRENCY,
) {
  return new Intl.NumberFormat(APP_LOCALE, {
    style: "currency",
    currency,
    currencyDisplay: "code",
  }).format(minor / 100);
}
