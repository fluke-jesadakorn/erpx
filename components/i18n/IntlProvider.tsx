'use client';

import React from 'react';
import { NextIntlClientProvider } from 'next-intl';
import { PRIMARY } from '@/i18n/config';
import enMessages from '../../messages/en.json';

export function IntlProvider({ children }: { children: React.ReactNode }) {
  return (
    <NextIntlClientProvider
      locale={PRIMARY}
      messages={enMessages}
      timeZone="Asia/Bangkok"
      now={new Date()}
    >
      {children}
    </NextIntlClientProvider>
  );
}
