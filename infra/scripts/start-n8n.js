#!/usr/bin/env node
// Wrapper to load .env and start n8n
// Used by launchd (avoids needing /bin/bash Full Disk Access — node already has it)

const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const ENV_FILE = '/Users/fluke/Desktop/Work/Contents/infra/.env';
const N8N_BIN = '/Users/fluke/.nvm/versions/node/v22.23.0/bin/n8n';

// Parse .env manually (KEY=VALUE, ignore comments/blanks, strip quotes)
const envContent = fs.readFileSync(ENV_FILE, 'utf8');
const env = { ...process.env };
for (const line of envContent.split('\n')) {
  const trimmed = line.trim();
  if (!trimmed || trimmed.startsWith('#')) continue;
  const eq = trimmed.indexOf('=');
  if (eq < 0) continue;
  const k = trimmed.slice(0, eq).trim();
  let v = trimmed.slice(eq + 1).trim();
  if ((v.startsWith('"') && v.endsWith('"')) || (v.startsWith("'") && v.endsWith("'"))) {
    v = v.slice(1, -1);
  }
  env[k] = v;
}

const child = spawn(N8N_BIN, ['start'], {
  env,
  stdio: 'inherit',
});

child.on('exit', (code, signal) => {
  process.exit(code ?? 1);
});
