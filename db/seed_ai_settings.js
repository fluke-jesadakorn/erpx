// Seed AI providers, models, staff, assignments, and the IT demo user.
// Run after add_ai_settings.sql + add_it_role.sql.
//   node db/seed_ai_settings.js
//
// Reads POSTGRES_* and ENCRYPTION_KEY from app/.env.local (no extra deps).

const fs = require('fs');
const path = require('path');
const { Pool } = require('pg');

function loadEnv(file) {
  if (!fs.existsSync(file)) return;
  for (const line of fs.readFileSync(file, 'utf8').split('\n')) {
    const m = line.match(/^\s*([A-Z_][A-Z0-9_]*)\s*=\s*(.*?)\s*$/i);
    if (!m) continue;
    if (process.env[m[1]] == null) process.env[m[1]] = m[2].replace(/^['"]|['"]$/g, '');
  }
}
loadEnv(path.resolve(__dirname, '../app/.env.local'));

const pool = new Pool({
  user: process.env.POSTGRES_USER,
  password: process.env.POSTGRES_PASSWORD,
  host: process.env.POSTGRES_HOST,
  database: process.env.POSTGRES_DB || 'folio_db',
  port: parseInt(process.env.POSTGRES_PORT, 10),
});

const ENC_KEY = process.env.ENCRYPTION_KEY;
if (!ENC_KEY) { console.error('ENCRYPTION_KEY missing in .env.local'); process.exit(1); }

async function q(sql, params) {
  const r = await pool.query(sql, params);
  return r.rows;
}

async function seed() {
  console.log('🔌 Seeding AI providers…');
  const openrouterKey = process.env.OPENROUTER_API_KEY || '';
  const minimaxKey = process.env.MINIMAX_API_KEY || '';
  const minimaxModel = process.env.MINIMAX_MODEL || 'MiniMax-M3';
  const minimaxBaseUrl = process.env.MINIMAX_BASE_URL || 'https://api.minimax.chat/v1';

  const ollama = (await q(
    `INSERT INTO ai_providers (name, type, base_url, enabled, notes)
     VALUES ('local-ollama', 'ollama', 'http://localhost:11434', true, 'Local Ollama daemon')
     ON CONFLICT (name) DO UPDATE SET base_url = EXCLUDED.base_url
     RETURNING id`
  ))[0];
  console.log(`  ✓ local-ollama (id=${ollama.id})`);

  if (openrouterKey) {
    const enc = (await q(`SELECT ai_encrypt($1, $2) AS enc`, [openrouterKey, ENC_KEY]))[0];
    const or = (await q(
      `INSERT INTO ai_providers (name, type, base_url, api_key_enc, enabled, preset, notes)
       VALUES ('openrouter', 'openai_compat', 'https://openrouter.ai/api/v1', $1, true, 'openrouter', 'OpenRouter gateway')
       ON CONFLICT (name) DO UPDATE SET api_key_enc = EXCLUDED.api_key_enc, base_url = EXCLUDED.base_url
       RETURNING id`,
      [enc.enc]
    ))[0];
    console.log(`  ✓ openrouter (id=${or.id}) — key encrypted`);
  } else {
    console.log('  ⚠ OPENROUTER_API_KEY missing — skipping openrouter provider');
  }

  if (minimaxKey) {
    const enc = (await q(`SELECT ai_encrypt($1, $2) AS enc`, [minimaxKey, ENC_KEY]))[0];
    const mm = (await q(
      `INSERT INTO ai_providers (name, type, base_url, api_key_enc, enabled, preset, notes)
       VALUES ('MiniMax', 'minimax', $1, $2, true, 'minimax', 'MiniMax M3 — bootstrap key from env, editable via UI')
       ON CONFLICT (name) DO UPDATE SET
         api_key_enc = EXCLUDED.api_key_enc,
         base_url = EXCLUDED.base_url
       RETURNING id`,
      [minimaxBaseUrl, enc.enc]
    ))[0];
    console.log(`  ✓ MiniMax (id=${mm.id}) — key encrypted from env`);
  } else {
    const mm = (await q(
      `INSERT INTO ai_providers (name, type, base_url, enabled, preset, notes)
       VALUES ('MiniMax', 'minimax', $1, true, 'minimax', 'MiniMax preset — enter API key via UI or MINIMAX_API_KEY env')
       ON CONFLICT (name) DO UPDATE SET base_url = EXCLUDED.base_url
       RETURNING id`,
      [minimaxBaseUrl]
    ))[0];
    console.log(`  ✓ MiniMax preset (id=${mm.id}) — UI prompts for API key`);
  }

  console.log('🧠 Seeding models…');
  const localModels = [
    { name: 'bge-m3:latest',  capabilities: ['embed'],           context: 8192,    defaults: {} },
    { name: 'qwen2.5:7b',      capabilities: ['chat'],            context: 32768,   defaults: { temperature: 0.3 } },
    { name: 'qwen3-vl:4b',     capabilities: ['chat', 'vision'],  context: 8192,    defaults: { temperature: 0.1 } },
    { name: 'llama3.2:latest', capabilities: ['chat'],            context: 8192,    defaults: { temperature: 0.5 } },
  ];
  for (const m of localModels) {
    await q(
      `INSERT INTO ai_models (provider_id, name, capabilities, context_window, defaults_json)
       VALUES ($1, $2, $3, $4, $5)
       ON CONFLICT (provider_id, name) DO UPDATE SET capabilities = EXCLUDED.capabilities`,
      [ollama.id, m.name, m.capabilities, m.context, m.defaults]
    );
  }
  console.log(`  ✓ ${localModels.length} models under local-ollama`);

  const mm = (await q(`SELECT id FROM ai_providers WHERE name='MiniMax'`))[0];
  if (mm) {
    const mmModels = [
      { name: minimaxModel, capabilities: ['chat', 'vision'], context: 128000, defaults: { temperature: 0.3, max_tokens: 2048 } },
    ];
    for (const m of mmModels) {
      await q(
        `INSERT INTO ai_models (provider_id, name, capabilities, context_window, defaults_json)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (provider_id, name) DO UPDATE SET
           capabilities = EXCLUDED.capabilities,
           context_window = EXCLUDED.context_window,
           defaults_json = EXCLUDED.defaults_json`,
        [mm.id, m.name, m.capabilities, m.context, m.defaults]
      );
    }
    console.log(`  ✓ ${mmModels.length} models under MiniMax`);
  }

  if (openrouterKey) {
    const or = (await q(`SELECT id FROM ai_providers WHERE name='openrouter'`))[0];
    const orModels = [
      { name: 'google/gemini-2.5-flash', capabilities: ['chat', 'vision'], context: 1000000, defaults: { temperature: 0.3 } },
    ];
    for (const m of orModels) {
      await q(
        `INSERT INTO ai_models (provider_id, name, capabilities, context_window, defaults_json)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (provider_id, name) DO UPDATE SET capabilities = EXCLUDED.capabilities`,
        [or.id, m.name, m.capabilities, m.context, m.defaults]
      );
    }
    console.log(`  ✓ ${orModels.length} models under openrouter`);
  }

  console.log('🤖 Seeding AI staff…');
  const localOllamaModel = (await q(
    `SELECT id FROM ai_models WHERE provider_id=$1 AND name='qwen2.5:7b'`, [ollama.id]
  ))[0];

  const staff = [
    {
      name: 'Accountant Reviewer',
      role_label: 'Accountant Helper',
      description: 'Helps review expense claims and suggest chart-of-account codes',
      system_prompt: 'You are an expert accountant who reviews expense claims and suggests correct chart-of-account codes. Respond in Thai when the input is Thai.',
      capabilities: ['chat'],
      default_model: localOllamaModel.id,
    },
    {
      name: 'Policy Drafter',
      role_label: 'Approval Policy Writer',
      description: 'Helps draft approval policies',
      system_prompt: 'You help draft approval policies for a finance system. Output JSON only with conditions_json and action_json structure.',
      capabilities: ['chat'],
      default_model: localOllamaModel.id,
    },
  ];

  if (openrouterKey) {
    const or = (await q(`SELECT id FROM ai_providers WHERE name='openrouter'`))[0];
    const gemini = (await q(
      `SELECT id FROM ai_models WHERE provider_id=$1 AND name='google/gemini-2.5-flash'`, [or.id]
    ))[0];
    staff.push(
      {
        name: 'OCR Clerk',
        role_label: 'Receipt Vision Specialist',
        description: 'Extract data from receipts (OCR)',
        system_prompt: 'You are a professional financial AI parsing agent. Analyze the receipt image and extract values into a single JSON object following the Folio slip schema.',
        capabilities: ['chat', 'vision'],
        default_model: gemini.id,
      },
      {
        name: 'CFO Narrator',
        role_label: 'Executive Narrative Writer',
        description: 'Executive summary narrative',
        system_prompt: 'You write a 2-paragraph executive narrative summarizing the company financial position for the CFO cockpit. Thai language, formal tone.',
        capabilities: ['chat'],
        default_model: gemini.id,
      }
    );
  }

  for (const s of staff) {
    await q(
      `INSERT INTO ai_staff (name, role_label, description, system_prompt, capabilities, default_provider_id, default_model_id)
       VALUES ($1,$2,$3,$4,$5,$6,$7)
       ON CONFLICT DO NOTHING`,
      [s.name, s.role_label, s.description, s.system_prompt, s.capabilities, ollama.id, s.default_model]
    );
  }
  console.log(`  ✓ ${staff.length} staff records`);

  console.log('🧭 Seeding section assignments…');
  const ollamaBge = (await q(`SELECT id FROM ai_models WHERE provider_id=$1 AND name='bge-m3:latest'`, [ollama.id]))[0];

  const assignments = [
    { section: 'acct:coa-search',    task: 'embed',  provider: ollama.id, model: ollamaBge.id },
  ];

  if (mm && minimaxKey) {
    const mmModel = (await q(
      `SELECT id FROM ai_models WHERE provider_id=$1 AND name=$2`, [mm.id, minimaxModel]
    ))[0];
    if (mmModel) {
      const mmAssignments = [
        { section: 'staff:ocr',          task: 'vision', provider: mm.id, model: mmModel.id },
        { section: 'staff:submit',       task: 'chat',   provider: mm.id, model: mmModel.id },
        { section: 'acct:queue',         task: 'chat',   provider: mm.id, model: mmModel.id },
        { section: 'hod:approve',        task: 'chat',   provider: mm.id, model: mmModel.id },
        { section: 'am:review',          task: 'chat',   provider: mm.id, model: mmModel.id },
        { section: 'cfo:cockpit',        task: 'chat',   provider: mm.id, model: mmModel.id },
        { section: 'ceo:cockpit',        task: 'chat',   provider: mm.id, model: mmModel.id },
        { section: 'ledger:commentary',  task: 'chat',   provider: mm.id, model: mmModel.id },
        { section: 'policy:editor',      task: 'chat',   provider: mm.id, model: mmModel.id },
        { section: 'command:intent',     task: 'chat',   provider: mm.id, model: mmModel.id },
        { section: 'notification:digest',task: 'chat',   provider: mm.id, model: mmModel.id },
      ];
      assignments.push(...mmAssignments);
      console.log(`  ✓ ${mmAssignments.length} sections → MiniMax/${minimaxModel}`);
    }
  }

  for (const a of assignments) {
    await q(
      `INSERT INTO ai_assignments (section_key, task_type, provider_id, model_id, priority, enabled)
       VALUES ($1,$2,$3,$4,100,true)
       ON CONFLICT (section_key, task_type, priority) DO UPDATE SET
         provider_id = EXCLUDED.provider_id,
         model_id = EXCLUDED.model_id,
         enabled = EXCLUDED.enabled,
         updated_at = NOW()`,
      [a.section, a.task, a.provider, a.model]
    );
  }
  console.log(`  ✓ ${assignments.length} assignments`);

  console.log('👤 Seeding IT demo user…');
  const itRole = (await q(`SELECT id FROM roles WHERE name='it'`))[0];
  if (!itRole) { console.error('  ✗ it role missing — run add_it_role.sql first'); process.exit(1); }

const existing = (await q(`SELECT id FROM users WHERE employee_code='IT001'`))[0];
  if (!existing) {
    await q(
      `INSERT INTO users (employee_code, fullname, role_id, department)
       VALUES ($1, $2, $3, $4)`,
      ['IT001', 'Alex Admin', itRole.id, 'IT']
    );
    console.log('  ✓ Alex Admin (IT001)');
  } else {
    await q(`UPDATE users SET role_id=$1, fullname='Alex Admin', department='IT' WHERE id=$2`, [itRole.id, existing.id]);
    console.log(`  ✓ Updated existing user #${existing.id} to Alex Admin / IT role`);
  }

  // Demo users for any role that has zero users — keeps the persona switcher fully populated.
  console.log('🧩 Filling missing role personas…');
  const mockUsers = [
    { code: 'EMP012', name: 'Daniel Accountant',  role: 'accountant',         dept: 'Finance & Account' },
    { code: 'EMP013', name: 'Michael Manager',     role: 'manager',            dept: 'Operations' },
    { code: 'EMP014', name: 'Brian Admin',  role: 'admin',              dept: 'Executive' },
  ];
  for (const m of mockUsers) {
    const roleRow = (await q(`SELECT id FROM roles WHERE name=$1`, [m.role]))[0];
    if (!roleRow) continue;
    const present = (await q(`SELECT id FROM users WHERE employee_code=$1`, [m.code]))[0];
    if (present) {
      await q(`UPDATE users SET role_id=$1, fullname=$2, department=$3 WHERE id=$4`,
        [roleRow.id, m.name, m.dept, present.id]);
      console.log(`  ↻ ${m.code} ${m.name} (${m.role})`);
    } else {
      await q(
        `INSERT INTO users (employee_code, fullname, role_id, department)
         VALUES ($1,$2,$3,$4)`,
        [m.code, m.name, roleRow.id, m.dept]
      );
      console.log(`  ✓ ${m.code} ${m.name} (${m.role})`);
    }
  }

  console.log('\n✅ Seed complete.');
  await pool.end();
}

seed().catch(e => { console.error(e); process.exit(1); });