// App Router topology, separate from source identity and content fingerprints.
// Named slots and route groups organize the loader tree; neither is a URL segment.
const appRoot = 'src/app';
const segmentConventions = ['layout', 'template', 'loading', 'error', 'not-found', 'forbidden', 'unauthorized'];

function pageSegments(source) {
  if (!source.startsWith(`${appRoot}/`) || !source.endsWith('/page.tsx')) return null;
  const segments = source.slice(appRoot.length + 1, -'/page.tsx'.length).split('/').filter(Boolean);
  if (segments.some(segment => segment.startsWith('_'))) return null;
  return segments;
}

function publicPath(segments) {
  const pathname = [];
  for (const segment of segments) {
    if (segment.startsWith('@') || /^\([^/]+\)$/.test(segment)) continue;
    // No current ERPX entry uses interception. Refuse an invented public URL if
    // one is added until its intercepted/host navigation contexts are inventoried.
    if (/^\(\.{1,3}\)/.test(segment)) throw new Error('UI_INTERCEPTED_ROUTE_CONTEXT_REQUIRED');
    pathname.push(segment.replace(/%5F/g, '_'));
  }
  return `/${pathname.join('/')}`;
}

export function appPageContext(source) {
  const segments = pageSegments(source);
  if (!segments) return null;
  const slots = [];
  for (let index = 0; index < segments.length; index++) {
    if (!segments[index].startsWith('@')) continue;
    const parent = segments.slice(0, index);
    slots.push({
      name: segments[index].slice(1),
      source: `${appRoot}/${segments.slice(0, index + 1).join('/')}`,
      parentSource: parent.length ? `${appRoot}/${parent.join('/')}` : appRoot,
      parentRoute: publicPath(parent),
    });
  }
  return { route: publicPath(segments), routeContext: { kind: slots.length ? 'parallel-slot' : 'children', slots } };
}

export function appPageBoundaries(source, availableSources) {
  const segments = pageSegments(source);
  if (!segments) return [];
  const available = new Set(availableSources), ancestors = new Set();
  for (let length = segments.length; length >= 0; length--) {
    ancestors.add(length ? `${appRoot}/${segments.slice(0, length).join('/')}` : appRoot);
  }
  const boundaries = new Set();
  for (const parent of ancestors) {
    for (const convention of segmentConventions) {
      const candidate = `${parent}/${convention}.tsx`;
      if (available.has(candidate)) boundaries.add(candidate);
    }
    // children is an implicit slot and may have its own hard-navigation fallback.
    const fallback = `${parent}/default.tsx`;
    if (available.has(fallback)) boundaries.add(fallback);
  }
  // A shared layout's adjacent named slots use default on an unmatched hard
  // navigation. Keep those actual fallback owners without importing every page
  // in every sibling slot into every route's source closure.
  for (const candidate of available) {
    const match = /^(.*)\/@[^/]+\/default\.tsx$/.exec(candidate);
    if (match && ancestors.has(match[1])) boundaries.add(candidate);
  }
  // Next resolves this at app root and supplies it to every loader-tree layer.
  if (available.has(`${appRoot}/global-error.tsx`)) boundaries.add(`${appRoot}/global-error.tsx`);
  return [...boundaries].sort();
}
