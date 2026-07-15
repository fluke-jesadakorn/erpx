import type { MessageDict } from './types';
import common from './common';
import nav from './nav';
import auth from './auth';
import chrome from './chrome';
import waybill from './waybill';
import finance from './finance';
import tiles from './tiles';
import permissions from './permissions';
import hr from './hr';
import ai from './ai';
import chat from './chat';
import onboarding from './onboarding';
import sales from './sales';
import customers from './customers';
import departments from './departments';

function merge(dicts: MessageDict[]): MessageDict {
  const en: Record<string, string> = {};
  const th: Record<string, string> = {};
  const de: Record<string, string> = {};
  for (const d of dicts) {
    Object.assign(en, d.en);
    if (d.th) Object.assign(th, d.th);
    if (d.de) Object.assign(de, d.de);
  }
  return { en, th, de };
}

export const ALL_MESSAGES = merge([
  common,
  nav,
  auth,
  chrome,
  waybill,
  finance,
  tiles,
  permissions,
  hr,
  ai,
  chat,
  onboarding,
  sales,
  customers,
  departments,
]);

export default ALL_MESSAGES;