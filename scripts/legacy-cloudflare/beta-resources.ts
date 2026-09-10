/** Generate/provision the isolated 20-company pool. No business seed is ever run. */
import { resourceSlots } from "../src/lib/holding-registry";
const remote = process.argv.includes("--remote"),
  provision = process.argv.includes("--provision"),
  initialize = process.argv.includes("--initialize");
const configPath = "wrangler.beta.jsonc",
  jobsPath = "wrangler.beta.jobs.jsonc";
const slots = Array.from({ length: 20 }, (_, i) => {
  const n = String(i + 1).padStart(3, "0");
  return {
    id: `beta${n}`,
    write: `B${n}_DB`,
    read: `B${n}_READ_DB`,
    files: `B${n}_FILES`,
    beta: true,
  };
});
function wrangler(args: string[], capture = false) {
  const result = Bun.spawnSync(["bunx", "wrangler", ...args], {
    stdout: capture ? "pipe" : "inherit",
    stderr: "inherit",
    env: { ...process.env, WRANGLER_SEND_METRICS: "false" },
  });
  if (result.exitCode)
    throw new Error(`Wrangler failed: ${args.slice(0, 3).join(" ")}`);
  return capture ? new TextDecoder().decode(result.stdout) : "";
}
const old = (await Bun.file(configPath).exists())
  ? JSON.parse(await Bun.file(configPath).text())
  : null;
const d1 = [
  {
    binding: "CONTROL_DB",
    database_name: "erpx-beta-control",
    migrations_dir: "migrations/control",
  },
  ...slots.flatMap((s) => [
    {
      binding: s.write,
      database_name: `erpx-${s.id}-write`,
      migrations_dir: "migrations/holding-write",
    },
    {
      binding: s.read,
      database_name: `erpx-${s.id}-read`,
      migrations_dir: "migrations/holding-read",
    },
  ]),
].map((d, i) => ({
  ...d,
  database_id:
    old?.d1_databases?.find((x: { binding: string }) => x.binding === d.binding)
      ?.database_id ??
    `00000000-0000-0000-0000-${String(i + 100).padStart(12, "0")}`,
}));
const buckets = slots.map((s) => ({
  binding: s.files,
  bucket_name: `erpx-${s.id}-files`,
}));
if (provision) {
  if (!remote)
    throw new Error(
      "--provision requires --remote. Use --initialize alone for local resources.",
    );
  const databases = JSON.parse(wrangler(["d1", "list", "--json"], true)) as {
    name: string;
    uuid: string;
  }[];
  for (const d of d1)
    if (!databases.some((x) => x.name === d.database_name))
      wrangler(["d1", "create", d.database_name]);
  const actual = JSON.parse(wrangler(["d1", "list", "--json"], true)) as {
    name: string;
    uuid: string;
  }[];
  for (const d of d1) {
    const found = actual.find((x) => x.name === d.database_name);
    if (!found) throw new Error("DATABASE_NOT_CREATED");
    d.database_id = found.uuid;
  }
  for (const b of buckets) {
    const r = Bun.spawnSync(
      ["bunx", "wrangler", "r2", "bucket", "create", b.bucket_name],
      { stdout: "pipe", stderr: "pipe" },
    );
    if (
      r.exitCode &&
      !/already exists|already.*own/i.test(
        new TextDecoder().decode(r.stderr) + new TextDecoder().decode(r.stdout),
      )
    )
      throw new Error(`Bucket creation failed: ${b.bucket_name}`);
  }
  for (const queue of ["erpx-beta-events", "erpx-beta-events-dlq"]) {
    const r = Bun.spawnSync(["bunx", "wrangler", "queues", "create", queue], {
      stdout: "pipe",
      stderr: "pipe",
    });
    if (
      r.exitCode &&
      !/already exists/i.test(
        new TextDecoder().decode(r.stderr) + new TextDecoder().decode(r.stdout),
      )
    )
      throw new Error(`Queue creation failed: ${queue}`);
  }
}
const vars = {
  ERPX_ENVIRONMENT: "beta",
  BETTER_AUTH_URL:
    process.env.ERPX_BETA_URL ??
    old?.vars?.BETTER_AUTH_URL ??
    "https://configure-your-beta-domain.example",
  ERPX_PUBLIC_BETA: old?.vars?.ERPX_PUBLIC_BETA ?? "false",
  ERPX_RESOURCE_SLOTS: JSON.stringify(slots),
  BETA_MEMBER_LIMIT: "20",
  BETA_BRANCH_LIMIT: "5",
  BETA_AI_LIMIT: "100",
  BETA_STORAGE_BYTES: String(1024 ** 3),
  ...(old?.vars ?? {}),
};
const common = {
  $schema: "./node_modules/wrangler/config-schema.json",
  compatibility_date: "2026-09-05",
  compatibility_flags: ["nodejs_compat", "global_fetch_strictly_public"],
  observability: { enabled: true },
  vars,
  d1_databases: d1,
  r2_buckets: buckets,
};
const app = {
  ...common,
  name: "erpx-beta",
  main: ".open-next/worker.js",
  assets: { directory: ".open-next/assets", binding: "ASSETS" },
  queues: {
    producers: [{ binding: "ERPX_EVENTS", queue: "erpx-beta-events" }],
  },
};
const jobs = {
  ...common,
  name: "erpx-beta-jobs",
  main: "src/workers/jobs.ts",
  triggers: { crons: ["*/2 * * * *"] },
  queues: {
    producers: app.queues.producers,
    consumers: [
      {
        queue: "erpx-beta-events",
        max_batch_size: 20,
        max_retries: 5,
        dead_letter_queue: "erpx-beta-events-dlq",
      },
    ],
  },
};
resourceSlots(vars);
await Bun.write(configPath, JSON.stringify(app, null, 2) + "\n");
await Bun.write(jobsPath, JSON.stringify(jobs, null, 2) + "\n");
if (initialize) {
  if (remote && d1.some((d) => d.database_id.startsWith("00000000-")))
    throw new Error("Provision actual databases first.");
  for (const d of d1)
    wrangler([
      "d1",
      "migrations",
      "apply",
      d.binding,
      "--config",
      configPath,
      remote ? "--remote" : "--local",
    ]);
  // Only resources with applied schemas can become reservable. Customer databases remain empty.
  for (const s of slots)
    wrangler([
      "d1",
      "execute",
      s.write,
      "--config",
      configPath,
      remote ? "--remote" : "--local",
      "--command",
      "SELECT count(*) AS initialized FROM beta_policy;",
    ]);
  const sql = slots
    .map(
      (s) =>
        `INSERT OR IGNORE INTO beta_slots(holding_id,created_at,updated_at) VALUES('${s.id}',unixepoch()*1000,unixepoch()*1000);`,
    )
    .join("\n");
  wrangler([
    "d1",
    "execute",
    "CONTROL_DB",
    "--config",
    configPath,
    remote ? "--remote" : "--local",
    "--command",
    sql,
  ]);
}
console.log(
  `Prepared ${slots.length} isolated write/read/file slots. Registration remains ${vars.ERPX_PUBLIC_BETA === "true" ? "enabled" : "closed"}.`,
);
