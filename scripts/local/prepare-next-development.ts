import { readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const legacyMarker = "/* ERPX: cyclic React development debug metadata */";
const marker = "/* ERPX: bounded cyclic React development debug metadata */";
const typeWriteMarker = "/* ERPX: atomic development route type writes */";

/** The dev watcher can overlap generations of routes.d.ts and validator.ts. */
export function guardDevelopmentTypeWrites(source: string): string {
  if (source.includes(typeWriteMarker)) return source;
  const routeWrite = "await (0, _routetypesutils.writeRouteTypesManifest)(routeTypesManifest, routeTypesFilePath, opts.nextConfig);";
  const validatorWrite = "await (0, _routetypesutils.writeValidatorFile)(routeTypesManifest, validatorFilePath, Boolean(nextConfig.experimental.strictRouteTypes));";
  const entry = "async function setupDevBundler(opts) {";
  for (const target of [routeWrite, validatorWrite, entry]) {
    if (source.split(target).length !== 2)
      throw new Error("Next development type-write guard no longer matches; review the pinned runtime.");
  }
  const helper = `${typeWriteMarker}
const erpxGeneratedTypeWrites = new Map();
async function erpxWriteGeneratedType(filePath, writer) {
    const previous = erpxGeneratedTypeWrites.get(filePath) || Promise.resolve();
    const pending = previous.catch(() => {}).then(async () => {
        const temporary = filePath + "." + process.pid + "." + require("node:crypto").randomUUID() + ".tmp";
        try {
            await writer(temporary);
            await _fs.default.promises.rename(temporary, filePath);
        } finally {
            await _fs.default.promises.unlink(temporary).catch(() => {});
        }
    });
    erpxGeneratedTypeWrites.set(filePath, pending);
    try { await pending; }
    finally {
        if (erpxGeneratedTypeWrites.get(filePath) === pending)
            erpxGeneratedTypeWrites.delete(filePath);
    }
}
/* ERPX: end atomic development route type writes */
`;
  return source
    .replace(entry, helper + entry)
    .replace(routeWrite, "await erpxWriteGeneratedType(routeTypesFilePath, temporary => (0, _routetypesutils.writeRouteTypesManifest)(routeTypesManifest, temporary, opts.nextConfig));")
    .replace(validatorWrite, "await erpxWriteGeneratedType(validatorFilePath, temporary => (0, _routetypesutils.writeValidatorFile)(routeTypesManifest, temporary, Boolean(nextConfig.experimental.strictRouteTypes)));");
}

/** Keep optional React development ancestry from exhausting request memory.
 * Ordinary traces retain their original index; page/model chunks are untouched. */
export function guardDebugChunkCycles(source: string): string {
  if (source.includes(marker)) return source;
  const previousGuard = `${legacyMarker}
      for (var idx = debugInfo.length, c = debugChunk._debugChunk,
        erpxSeenDebugChunks = new Set([debugChunk]); null !== c;) {
        if (erpxSeenDebugChunks.has(c)) {
          // Debug metadata is optional; leave the actual model untouched.
          chunk._debugChunk = null;
          return;
        }
        erpxSeenDebugChunks.add(c);
        "fulfilled" !== c.status && idx++, (c = c._debugChunk);
      }`;
  const boundedGuard = `${marker}
      for (var idx = debugInfo.length, c = debugChunk._debugChunk,
        erpxSeenDebugChunks = new Set([debugChunk]); null !== c;) {
        if (erpxSeenDebugChunks.size >= 4096 || erpxSeenDebugChunks.has(c)) {
          // Bound optional ancestry as well as cycles; keep the page model intact.
          chunk._debugChunk = null;
          return;
        }
        erpxSeenDebugChunks.add(c);
        "fulfilled" !== c.status && idx++, (c = c._debugChunk);
      }`;
  if (source.includes(legacyMarker)) {
    if (source.split(previousGuard).length !== 2)
      throw new Error("Next development debug-cycle guard no longer matches; review the pinned runtime.");
    return source.replace(previousGuard, boundedGuard);
  }
  const loop = /for\s*\(\s*var idx\s*=\s*debugInfo\.length,\s*c\s*=\s*debugChunk\._debugChunk;\s*null\s*!==\s*c;\s*\)\s*"fulfilled"\s*!==\s*c.status\s*&&\s*idx\+\+,\s*\(?c\s*=\s*c\._debugChunk\)?;/g;
  let matches = 0;
  const patched = source.replace(loop, () => { matches++; return boundedGuard; });
  if (matches !== 1) throw new Error("Next development debug-cycle guard no longer matches; review the pinned runtime.");
  return patched;
}

/** Reapply after frozen installs; never modify production runtime files. */
export function prepareNextDevelopment(root = process.cwd()) {
  const next = join(root, "node_modules/next");
  const version = JSON.parse(readFileSync(join(next, "package.json"), "utf8")).version;
  if (version !== "16.4.0") throw new Error("Review the development debug-cycle workaround when changing Next 16.4.0.");
  const compiled = join(next, "dist/compiled");
  const files = readdirSync(join(compiled, "next-server"))
    .filter(name => /^app-page.*\.runtime\.dev\.js$/.test(name))
    .map(name => join(compiled, "next-server", name));
  for (const directory of readdirSync(compiled).filter(name => name.startsWith("react-server-dom-"))) {
    const cjs = join(compiled, directory, "cjs");
    for (const name of readdirSync(cjs).filter(name => /client\..*\.development\.js$/.test(name))) {
      const file = join(cjs, name);
      if (readFileSync(file, "utf8").includes("function initializeDebugChunk")) files.push(file);
    }
  }
  // Validate every target before writing any file.
  const patches = files.map(file => {
    const source = readFileSync(file, "utf8");
    return { file, source, patched: guardDebugChunkCycles(source) };
  });
  const devBundler = join(next, "dist/server/lib/router-utils/setup-dev-bundler.js");
  const devBundlerSource = readFileSync(devBundler, "utf8");
  patches.push({ file: devBundler, source: devBundlerSource, patched: guardDevelopmentTypeWrites(devBundlerSource) });
  for (const { file, source, patched } of patches) {
    if (source !== patched) writeFileSync(file, patched);
  }
  if (patches.some(({ source, patched }) => source !== patched)) {
    // Webpack treats node_modules as immutable, so its existing cache would
    // otherwise continue serving the unpatched browser/server vendor chunks.
    rmSync(join(root, process.env.ERPX_BUILD_DIR ?? ".next", "dev/cache"), { recursive: true, force: true });
  }
}
