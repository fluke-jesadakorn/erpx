import type { BrowserContext, Page } from '@playwright/test';

const LOCALE = 'th';

export async function pinLocale(ctx: BrowserContext, locale: 'th' | 'de' = LOCALE): Promise<void> {
  await ctx.addCookies([
    {
      name: 'folio.locale',
      value: locale,
      domain: 'localhost',
      path: '/',
      httpOnly: false,
      sameSite: 'Lax',
    },
  ]);
  await ctx.addInitScript((l: string) => {
    try {
      localStorage.setItem('folio.lang', l);
      localStorage.setItem('onboarded_v1', '1');
    } catch {}
    try {
      document.cookie = `folio.locale=${l}; Path=/; Max-Age=31536000; SameSite=Lax`;
    } catch {}
  }, locale);
}

export async function setLang(page: Page, locale: 'th' | 'de' | 'en'): Promise<void> {
  await page.evaluate((l: string) => {
    try {
      localStorage.setItem('folio.lang', l);
      document.cookie = `folio.locale=${l}; Path=/; Max-Age=31536000; SameSite=Lax`;
    } catch {}
  }, locale);
}
