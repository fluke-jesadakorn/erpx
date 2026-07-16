import { getRequestConfig } from 'next-intl/server';
import { PRIMARY } from './config';

export default getRequestConfig(async () => ({
  locale: PRIMARY,
  messages: (await import(`../../messages/${PRIMARY}.json`)).default,
}));
