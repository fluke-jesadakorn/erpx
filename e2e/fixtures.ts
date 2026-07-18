/* eslint-disable react-hooks/rules-of-hooks */
import { test as base, expect, type Page, type BrowserContext } from '@playwright/test';
import { pinLocale } from './helpers/lang';
import { signInOnPage } from './helpers/waybill';

export interface Persona {
  code: string;
  role: string;
  label: string;
}

export const PERSONAS = {
  officerEmp001: { code: 'EMP001', role: 'officer', label: 'John Staff (officer, dev)' },
  managerEmp002: { code: 'EMP002', role: 'manager', label: 'Sarah Approver (manager, dev)' },
  accountOfficerEmp003: { code: 'EMP003', role: 'account_officer', label: 'Mark Reviewer' },
  accountingManagerEmp004: { code: 'EMP004', role: 'accounting_manager', label: 'Emily Manager' },
  cfoEmp005: { code: 'EMP005', role: 'cfo', label: 'Olivia Director (CFO)' },
  ceoEmp006: { code: 'EMP006', role: 'ceo', label: 'Charles Executive (CEO)' },
  supervisorEmp017: { code: 'EMP017', role: 'supervisor', label: 'Steven Supervisor (dev)' },
  accountSupervisorEmp018: { code: 'EMP018', role: 'account_supervisor', label: 'Andrew Supervisor' },
  financeEmp019: { code: 'EMP019', role: 'finance', label: 'Tina Treasurer' },
  hrManagerEmp015: { code: 'EMP015', role: 'hr_manager', label: 'Patricia Manager (HR)' },
  hrOfficerEmp016: { code: 'EMP016', role: 'hr', label: 'Jennifer Staff (HR)' },
  itAdminIT001: { code: 'IT001', role: 'it', label: 'Alex Admin (IT)' },
  hrManagerHR002: { code: 'HR002', role: 'hr_manager', label: 'Pongsak HR-Manager' },
  lawParalegalLW001: { code: 'LW001', role: 'law', label: 'Wichai Paralegal' },
  lawCounselLW002: { code: 'LW002', role: 'counsel', label: 'Kornkrit Counsel' },
  salesOfficer: { code: 'EMP007', role: 'officer', label: 'Lisa Staff (marketing)' },
} as const;

export type PersonaKey = keyof typeof PERSONAS;

export interface FolioFixtures {
  ctx: BrowserContext;
  page: Page;
  persona: (key: PersonaKey) => Promise<void>;
  withPersona: <T>(key: PersonaKey, fn: (page: Page) => Promise<T>) => Promise<T>;
}

export const test = base.extend<FolioFixtures>({
  ctx: async ({ browser }, use) => {
    const ctx = await browser.newContext();
    await pinLocale(ctx, 'th');
    await use(ctx);
    await ctx.close();
  },
  page: async ({ ctx }, use) => {
    const page = await ctx.newPage();
    await use(page);
  },
  persona: async ({ page }, use) => {
    const fn = async (key: PersonaKey) => {
      const p = PERSONAS[key];
      await signInOnPage(page, p.code);
    };
    await use(fn);
  },
  withPersona: async ({ page }, use) => {
    const fn = async <T>(key: PersonaKey, body: (page: Page) => Promise<T>): Promise<T> => {
      const p = PERSONAS[key];
      await signInOnPage(page, p.code);
      return body(page);
    };
    await use(fn);
  },
});

export { expect };

export async function waitReady(page: Page): Promise<void> {
  await page.waitForLoadState('networkidle');
}
