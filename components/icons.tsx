import React from 'react';

export type IconName =
  | 'inbox' | 'bell' | 'check' | 'check-circle' | 'x' | 'arrow-left' | 'arrow-right'
  | 'arrow-up' | 'arrow-down' | 'chevron-down' | 'chevron-up' | 'chevron-right' | 'chevron-left'
  | 'menu' | 'search' | 'settings' | 'plus' | 'minus' | 'edit' | 'trash' | 'send'
  | 'lock' | 'unlock' | 'eye' | 'eye-off' | 'user' | 'users' | 'home' | 'star'
  | 'grid' | 'list' | 'filter' | 'sort' | 'calendar' | 'clock' | 'mail'
  | 'phone' | 'globe' | 'external-link' | 'copy' | 'download' | 'upload'
  | 'file' | 'folder' | 'image' | 'video' | 'paperclip' | 'link'
  | 'alert' | 'alert-circle' | 'info' | 'help' | 'loader' | 'refresh'
  | 'sun' | 'moon' | 'monitor' | 'smartphone' | 'database' | 'server'
  | 'shield' | 'shield-check' | 'key' | 'bookmark' | 'tag' | 'flag'
  | 'trending-up' | 'trending-down' | 'bar-chart' | 'pie-chart' | 'activity'
  | 'briefcase' | 'credit-card' | 'dollar-sign' | 'percent' | 'hash'
  | 'package' | 'truck' | 'shopping-cart' | 'gift' | 'award' | 'book-open'
  | 'command' | 'mouse-pointer' | 'crosshair' | 'zap' | 'cpu' | 'code'
  | 'gauge' | 'history' | 'layers' | 'building' | 'file-text' | 'receipt' | 'user-check';

const ICONS: Record<IconName, string> = {
  inbox: 'M22 12h-6l-2 3h-4l-2-3H2',
  bell: 'M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9',
  check: 'M20 6 9 17l-5-5',
  'check-circle': 'M22 11.08V12a10 10 0 1 1-5.93-9.14',
  x: 'M18 6 6 18 M6 6l12 12',
  'arrow-left': 'M19 12H5 M12 19l-7-7 7-7',
  'arrow-right': 'M5 12h14 M12 5l7 7-7 7',
  'arrow-up': 'M12 19V5 M5 12l7-7 7 7',
  'arrow-down': 'M12 5v14 M19 12l-7 7-7-7',
  'chevron-down': 'm6 9 6 6 6-6',
  'chevron-up': 'm18 15-6-6-6 6',
  'chevron-right': 'm9 18 6-6-6-6',
  'chevron-left': 'm15 18-6-6 6-6',
  menu: 'M3 12h18 M3 6h18 M3 18h18',
  search: 'M11 19a8 8 0 1 0 0-16 8 8 0 0 0 0 16z M21 21l-4.35-4.35',
  settings: 'M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z',
  plus: 'M12 5v14 M5 12h14',
  minus: 'M5 12h14',
  edit: 'M12 20h9 M16.5 3.5a2.121 2.121 0 1 1 3 3L7 19l-4 1 1-4z',
  trash: 'M3 6h18 M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6 M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2',
  send: 'M22 2 11 13 M22 2l-7 20-4-9-9-4z',
  lock: 'M5 11h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2z M7 11V7a5 5 0 0 1 10 0v4',
  unlock: 'M5 11h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2z M7 11V7a5 5 0 0 1 9.9-1',
  eye: 'M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6z',
  'eye-off': 'M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94 M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19 M1 1l22 22',
  user: 'M20 21v-2a4 4 0 0 0-4-4H8a4 4 0 0 0-4 4v2 M12 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8z',
  users: 'M17 21v-2a4 4 0 0 0-4-4H5a4 4 0 0 0-4 4v2 M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8z M23 21v-2a4 4 0 0 0-3-3.87 M16 3.13a4 4 0 0 1 0 7.75',
  home: 'M3 12 12 3l9 9 M5 10v10a1 1 0 0 0 1 1h3v-6h6v6h3a1 1 0 0 0 1-1V10',
  star: 'M12 2l3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01z',
  grid: 'M3 3h7v7H3z M14 3h7v7h-7z M14 14h7v7h-7z M3 14h7v7H3z',
  list: 'M8 6h13 M8 12h13 M8 18h13 M3 6h.01 M3 12h.01 M3 18h.01',
  filter: 'M22 3H2l8 9.46V19l4 2v-8.54z',
  sort: 'M11 5h10 M11 9h7 M11 13h4 M3 17l3 3 3-3 M6 18V4',
  calendar: 'M19 4H5a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h14a2 2 0 0 0 2-2V6a2 2 0 0 0-2-2z M16 2v4 M8 2v4 M3 10h18',
  clock: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z M12 6v6l4 2',
  mail: 'M4 4h16a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2z M22 6l-10 7L2 6',
  phone: 'M22 16.92v3a2 2 0 0 1-2.18 2 19.79 19.79 0 0 1-8.63-3.07 19.5 19.5 0 0 1-6-6 19.79 19.79 0 0 1-3.07-8.67A2 2 0 0 1 4.11 2h3a2 2 0 0 1 2 1.72 12.84 12.84 0 0 0 .7 2.81 2 2 0 0 1-.45 2.11L8.09 9.91a16 16 0 0 0 6 6l1.27-1.27a2 2 0 0 1 2.11-.45 12.84 12.84 0 0 0 2.81.7A2 2 0 0 1 22 16.92z',
  globe: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z M2 12h20 M12 2a15.3 15.3 0 0 1 4 10 15.3 15.3 0 0 1-4 10 15.3 15.3 0 0 1-4-10 15.3 15.3 0 0 1 4-10z',
  'external-link': 'M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6 M15 3h6v6 M10 14L21 3',
  copy: 'M20 9h-9a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2v-9a2 2 0 0 0-2-2z M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1',
  download: 'M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4 M7 10l5 5 5-5 M12 15V3',
  upload: 'M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4 M17 8l-5-5-5 5 M12 3v12',
  file: 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6',
  folder: 'M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z',
  image: 'M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4 M17 8l-5-5-5 5 M3 7h18v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z M8.5 10a1.5 1.5 0 1 0 0-3 1.5 1.5 0 0 0 0 3z',
  video: 'M23 7l-7 5 7 5V7z M14 5H3a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2z',
  paperclip: 'M21.44 11.05l-9.19 9.19a6 6 0 0 1-8.49-8.49l9.19-9.19a4 4 0 0 1 5.66 5.66l-9.2 9.19a2 2 0 0 1-2.83-2.83l8.49-8.48',
  link: 'M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71 M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71',
  alert: 'M12 9v4 M12 17h.01 M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z',
  'alert-circle': 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z M12 8v4 M12 16h.01',
  info: 'M12 22a10 10 0 1 0 0-20 10 10 0 0 0 0 20z M12 16v-4 M12 8h.01',
  help: 'M9.09 9a3 3 0 0 1 5.83 1c0 2-3 3-3 3 M12 17h.01 M22 12a10 10 0 1 1-20 0 10 10 0 0 1 20 0z',
  loader: 'M12 2v4 M12 18v4 M4.93 4.93l2.83 2.83 M16.24 16.24l2.83 2.83 M2 12h4 M18 12h4 M4.93 19.07l2.83-2.83 M16.24 7.76l2.83-2.83',
  refresh: 'M23 4v6h-6 M1 20v-6h6 M3.51 9a9 9 0 0 1 14.85-3.36L23 10 M1 14l4.64 4.36A9 9 0 0 0 20.49 15',
  sun: 'M12 17a5 5 0 1 0 0-10 5 5 0 0 0 0 10z M12 1v2 M12 21v2 M4.22 4.22l1.42 1.42 M18.36 18.36l1.42 1.42 M1 12h2 M21 12h2 M4.22 19.78l1.42-1.42 M18.36 5.64l1.42-1.42',
  moon: 'M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z',
  monitor: 'M20 3H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h7v2H8v2h8v-2h-3v-2h7a2 2 0 0 0 2-2V5a2 2 0 0 0-2-2z',
  smartphone: 'M17 2H7a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V4a2 2 0 0 0-2-2z M12 18h.01',
  database: 'M5 12s1.5-2 7-2 7 2 7 2-1.5 2-7 2-7-2-7-2z M5 6s1.5-2 7-2 7 2 7 2-1.5 2-7 2-7-2-7-2z M5 12v6c0 1.66 3.13 3 7 3s7-1.34 7-3v-6',
  server: 'M5 2h14a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2z M5 15h14a2 2 0 0 1 2 2v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-3a2 2 0 0 1 2-2z M7 6h.01 M7 19h.01',
  shield: 'M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z',
  'shield-check': 'M12 22s8-4 8-10V5l-8-3-8 3v7c0 6 8 10 8 10z M9 12l2 2 4-4',
  key: 'M21 2l-2 2m-7.61 7.61a5.5 5.5 0 1 1-7.778 7.778 5.5 5.5 0 0 1 7.777-7.777zm0 0L15.5 7.5m0 0l3 3L22 7l-3-3m-3.5 3.5L19 4',
  bookmark: 'M19 21l-7-5-7 5V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z',
  tag: 'M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z M7 7h.01',
  flag: 'M4 15s1-1 4-1 5 2 8 2 4-1 4-1V3s-1 1-4 1-5-2-8-2-4 1-4 1z M4 22V15',
  'trending-up': 'M23 6l-9.5 9.5-5-5L1 18 M17 6h6v6',
  'trending-down': 'M23 18l-9.5-9.5-5 5L1 6 M17 18h6v-6',
  'bar-chart': 'M12 20V10 M18 20V4 M6 20v-6',
  'pie-chart': 'M21 15.5A9 9 0 1 1 8.5 3 7 7 0 0 0 21 15.5z',
  gauge: 'M12 14l4-4 M3 12a9 9 0 0 1 18 0 M3 12a9 9 0 0 0 9 9 9 9 0 0 0 9-9',
  history: 'M1 4v6h6 M3.51 15a9 9 0 1 0 2.13-9.36L1 10 M12 7v5l4 2',
  layers: 'M12 2 2 7l10 5 10-5-10-5z M2 17l10 5 10-5 M2 12l10 5 10-5',
  activity: 'M22 12h-4l-3 9L9 3l-3 9H2',
  briefcase: 'M20 7H4a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2z M16 21V5a2 2 0 0 0-2-2h-4a2 2 0 0 0-2 2v16',
  'credit-card': 'M2 5h20a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H2a2 2 0 0 1-2-2V7a2 2 0 0 1 2-2z M2 10h20',
  'dollar-sign': 'M12 1v22 M17 5H9.5a3.5 3.5 0 0 0 0 7h5a3.5 3.5 0 0 1 0 7H6',
  percent: 'M19 5L5 19 M6.5 6.5a2.12 2.12 0 1 0 0 3 M17.5 17.5a2.12 2.12 0 1 0 0-3',
  hash: 'M4 9h16 M4 15h16 M10 3 8 21 M16 3l-2 18',
  package: 'M16.5 9.4 7.55 4.24 M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z M3.27 6.96 12 12.01l8.73-5.05 M12 22.08V12',
  truck: 'M1 3h15v13H1z M16 8h4l3 3v5h-7 M5.5 18.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5z M18.5 18.5a2.5 2.5 0 1 0 0 5 2.5 2.5 0 0 0 0-5z',
  'shopping-cart': 'M9 21a1 1 0 1 0 0 2 1 1 0 0 0 0-2z M20 21a1 1 0 1 0 0 2 1 1 0 0 0 0-2z M1 1h4l2.68 13.39a2 2 0 0 0 2 1.61h9.72a2 2 0 0 0 2-1.61L23 6H6',
  gift: 'M20 12v10H4V12 M2 7h20v5H2z M12 22V7 M12 7H7.5a2.5 2.5 0 0 1 0-5C11 2 12 7 12 7z M12 7h4.5a2.5 2.5 0 0 0 0-5C13 2 12 7 12 7z',
  award: 'M12 15a6 6 0 1 0 0-12 6 6 0 0 0 0 12z M8.21 13.89L7 23l5-3 5 3-1.21-9.11',
  'book-open': 'M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z',
  command: 'M18 3a3 3 0 0 0-3 3v12a3 3 0 0 0 3 3 3 3 0 0 0 3-3 3 3 0 0 0-3-3H6a3 3 0 0 0-3 3 3 3 0 0 0 3 3 3 3 0 0 0 3-3V6a3 3 0 0 0-3-3 3 3 0 0 0-3 3 3 3 0 0 0 3 3h12a3 3 0 0 0 3-3 3 3 0 0 0-3-3z',
  'mouse-pointer': 'M3 3l7.07 16.97 2.51-7.39 7.39-2.51z M13 13l6-6',
  crosshair: 'M12 2v4 M12 18v4 M4.93 4.93l2.83 2.83 M16.24 16.24l2.83 2.83 M2 12h4 M18 12h4 M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20z',
  zap: 'M13 2 3 14h9l-1 8 10-12h-9z',
  cpu: 'M4 4h16v16H4z M9 9h6v6H9z M9 1v3 M15 1v3 M9 20v3 M15 20v3 M20 9h3 M20 14h3 M1 9h3 M1 14h3',
  code: 'M16 18l6-6-6-6 M8 6l-6 6 6 6',
  building: 'M3 21h18 M5 21V7l8-4v18 M19 21V11l-6-4 M9 9h.01 M9 12h.01 M9 15h.01 M9 18h.01',
  'file-text': 'M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z M14 2v6h6 M16 13H8 M16 17H8 M10 9H8',
  receipt: 'M5 21V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v16l-2-1-2 1-2-1-2 1-2-1-2 1-2-1z M9 7h6 M9 11h6 M9 15h4',
  'user-check': 'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2 M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8z M17 11l2 2 4-4',
};

interface IconProps extends React.SVGProps<SVGSVGElement> {
  name: IconName;
  size?: number;
}

export function Icon({ name, size = 16, className, ...rest }: IconProps) {
  const path = ICONS[name];
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      aria-hidden
      {...rest}
    >
      <path d={path} />
    </svg>
  );
}

const ROLE_ICONS: Record<string, IconName> = {
  ceo: 'star',
  cfo: 'briefcase',
  admin: 'shield',
  manager: 'users',
  supervisor: 'eye',
  account_supervisor: 'check-circle',
  account_officer: 'check',
  accounting_manager: 'bar-chart',
  hr_manager: 'users',
  hr: 'user',
  it: 'cpu',
  finance: 'dollar-sign',
  staff: 'user',
  officer: 'user',
  sales_rep: 'shopping-cart',
  sales_supervisor: 'trending-up',
  head_of_department: 'briefcase',
};

export function roleIcon(role?: string | null): IconName {
  if (!role) return 'user';
  const key = role.indexOf('::') >= 0 ? role.slice(0, role.indexOf('::')) : role;
  return ROLE_ICONS[key] ?? 'user';
}