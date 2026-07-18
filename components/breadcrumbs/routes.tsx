import { T } from '@/components/i18n/TServer';
import type { SecondaryLocale } from '@/i18n/config';
import type { Crumb } from '../breadcrumbs';

export interface CrumbContext {
  waybillId?: string;
  subtab?: 'gl' | 'audit' | 'chat' | 'attachments' | 'overview';
  customerCode?: string;
  lawDocId?: string;
  lawDocName?: string;
  salesId?: string;
  hrLeaveId?: string;
}

type Builder = (locale: SecondaryLocale, path: string, ctx: CrumbContext) => Crumb[];

const lbl = (id: string, locale: SecondaryLocale) => (<T id={id} locale={locale} />);

interface Route {
  test: (path: string) => boolean;
  build: Builder;
}

const ROUTES: Route[] = [
  {
    test: (p) => p === '/' || p === '',
    build: (locale, _path) => [homeCrumbFor(locale)],
  },
  {
    test: (p) => p === '/cockpit',
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('nav.cockpit', locale), href: '/cockpit' }],
  },
  {
    test: (p) => p === '/tiles',
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('tiles.pageLabel', locale), href: '/tiles' }],
  },
  {
    test: (p) => p === '/policy',
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('nav.policy', locale), href: '/policy' }],
  },
  {
    test: (p) => p === '/audit',
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('nav.audit', locale), href: '/audit' }],
  },
  {
    test: (p) => p === '/ai-settings',
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('nav.aiSettings', locale), href: '/ai-settings' }],
  },
  {
    test: (p) => p === '/chat',
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('nav.chat', locale), href: '/chat', icon: 'MessageCircle' }],
  },
  {
    test: (p) => p === '/nudges',
    build: (locale, _path) => [
      homeCrumbFor(locale),
      { label: lbl('waybill.nudges.title', locale), href: '/nudges' },
    ],
  },
  {
    test: (p) => p === '/inbox',
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('nav.inbox', locale), href: '/inbox' }],
  },
  {
    test: (p) => p === '/expense' || p.startsWith('/expense?'),
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('nav.expense', locale), href: '/expense' }],
  },
  {
    test: (p) => p === '/pr' || p.startsWith('/pr?'),
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('nav.pr', locale), href: '/pr' }],
  },
  {
    test: (p) => p === '/po' || p.startsWith('/po?'),
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('nav.po', locale), href: '/po' }],
  },
  {
    test: (p) => p === '/sales',
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('nav.sales', locale), href: '/sales' }],
  },
  {
    test: (p) => /^\/sales\/[^/]+$/.test(p),
    build: (locale, path, ctx) => [
      homeCrumbFor(locale),
      { label: lbl('nav.sales', locale), href: '/sales' },
      { label: ctx.salesId ?? '', href: path },
    ],
  },
  {
    test: (p) => p === '/customers',
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('nav.customers', locale), href: '/customers' }],
  },
  {
    test: (p) => /^\/customers\/[^/]+$/.test(p),
    build: (locale, path, ctx) => [
      homeCrumbFor(locale),
      { label: lbl('nav.customers', locale), href: '/customers' },
      { label: ctx.customerCode ?? '', href: path },
    ],
  },
  {
    test: (p) => /^\/waybill\/by-(expense|pr|po)\/[^/]+$/.test(p),
    build: (locale, _path) => [
      homeCrumbFor(locale),
      { label: lbl('waybill.inbox.title', locale), href: '/inbox' },
    ],
  },
  {
    test: (p) => /^\/waybill\/[^/]+$/.test(p),
    build: (locale, path, ctx) => [
      homeCrumbFor(locale),
      { label: lbl('waybill.inbox.title', locale), href: '/inbox' },
      { label: ctx.waybillId ?? '', href: path },
    ],
  },
  {
    test: (p) => /^\/waybill\/[^/]+\/(gl|audit|chat|attachments)$/.test(p),
    build: (locale, _path, ctx) => {
      const sub = ctx.subtab ?? 'overview';
      const subLabels: Record<string, string> = {
        gl: 'waybill.tab.gl',
        audit: 'waybill.tab.audit',
        chat: 'waybill.tab.chat',
        attachments: 'waybill.tab.attachments',
        overview: 'waybill.tab.overview',
      };
      return [
        homeCrumbFor(locale),
        { label: lbl('waybill.inbox.title', locale), href: '/inbox' },
        { label: ctx.waybillId ?? '', href: `/waybill/${ctx.waybillId ?? ''}` },
        { label: lbl(subLabels[sub] ?? sub, locale) },
      ];
    },
  },
  {
    test: (p) => p === '/me',
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('nav.me', locale), href: '/me' }],
  },
  {
    test: (p) => p === '/me/leave',
    build: (locale, _path) => [
      homeCrumbFor(locale),
      { label: lbl('nav.me', locale), href: '/me' },
      { label: lbl('me.leave.title', locale), href: '/me/leave' },
    ],
  },
  {
    test: (p) => p === '/hr',
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('nav.hr', locale), href: '/hr' }],
  },
  {
    test: (p) => p === '/hr/employees',
    build: (locale, _path) => [
      homeCrumbFor(locale),
      { label: lbl('nav.hr', locale), href: '/hr' },
      { label: lbl('hr.employees.title', locale), href: '/hr/employees' },
    ],
  },
  {
    test: (p) => p === '/hr/leave',
    build: (locale, _path) => [
      homeCrumbFor(locale),
      { label: lbl('nav.hr', locale), href: '/hr' },
      { label: lbl('hr.leave.title', locale), href: '/hr/leave' },
    ],
  },
  {
    test: (p) => /^\/hr\/leave\/[^/]+$/.test(p),
    build: (locale, path, ctx) => [
      homeCrumbFor(locale),
      { label: lbl('nav.hr', locale), href: '/hr' },
      { label: lbl('hr.leave.title', locale), href: '/hr/leave' },
      { label: ctx.hrLeaveId ?? '', href: path },
    ],
  },
  {
    test: (p) => p === '/law',
    build: (locale, _path) => [homeCrumbFor(locale), { label: lbl('law.title', locale), href: '/law' }],
  },
  {
    test: (p) => p === '/law/admin',
    build: (locale, _path) => [
      homeCrumbFor(locale),
      { label: lbl('law.title', locale), href: '/law' },
      { label: lbl('law.admin', locale), href: '/law/admin' },
    ],
  },
  {
    test: (p) => p === '/law/upload',
    build: (locale, _path) => [
      homeCrumbFor(locale),
      { label: lbl('law.title', locale), href: '/law' },
      { label: lbl('law.upload', locale), href: '/law/upload' },
    ],
  },
  {
    test: (p) => /^\/law\/[^/]+$/.test(p),
    build: (locale, path, ctx) => [
      homeCrumbFor(locale),
      { label: lbl('law.title', locale), href: '/law' },
      { label: ctx.lawDocName ?? ctx.lawDocId ?? '', href: path },
    ],
  },
  {
    test: (p) => /^\/law\/[^/]+\/(parties|chunks|metadata|audit|chat)$/.test(p),
    build: (locale, path, ctx) => {
      const m = path.match(/^\/law\/([^/]+)\/([^/]+)$/);
      const docId = m?.[1] ?? ctx.lawDocId ?? '';
      const sub = m?.[2] ?? '';
      const subLabels: Record<string, string> = {
        parties: 'law.tab.parties',
        chunks: 'law.tab.chunks',
        metadata: 'law.tab.metadata',
        audit: 'law.tab.audit',
        chat: 'law.tab.chat',
      };
      return [
        homeCrumbFor(locale),
        { label: lbl('law.title', locale), href: '/law' },
        { label: ctx.lawDocName ?? docId, href: `/law/${docId}` },
        { label: lbl(subLabels[sub] ?? sub, locale) },
      ];
    },
  },
  {
    test: (p) => /^\/[^/]+$/.test(p),
    build: (locale, path, _ctx) => [
      homeCrumbFor(locale),
      { label: path.replace(/^\//, ''), href: path },
    ],
  },
];

function homeCrumbFor(locale: SecondaryLocale): Crumb {
  return {
    label: <T id="breadcrumbs.home" locale={locale} />,
    href: '/',
    icon: 'Home',
  };
}

export function crumbsForPath(
  pathname: string,
  locale: SecondaryLocale,
  ctx: CrumbContext = {},
): Crumb[] {
  const path = pathname.split('?')[0];
  for (const route of ROUTES) {
    if (route.test(path)) return route.build(locale, path, ctx);
  }
  return [homeCrumbFor(locale)];
}

export function trail(
  locale: SecondaryLocale,
  ...crumbs: Crumb[]
): Crumb[] {
  return [homeCrumbFor(locale), ...crumbs];
}
