#!/usr/bin/env node
// Build the admin SPA and copy its dist into the `cloakpipe` npm package's
// `public/` directory so `npx cloakpipe serve` ships the compiled assets.
import { execSync } from 'node:child_process';
import { cpSync, existsSync, rmSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const uiDir = resolve(root, 'apps/admin-ui');
const dist = resolve(uiDir, 'dist');
const target = resolve(root, 'packages/cloakpipe-serve/public');

function run(cmd, cwd) {
  execSync(cmd, { cwd, stdio: 'inherit' });
}

if (!existsSync(resolve(uiDir, 'node_modules'))) {
  run('npm install', uiDir);
}
run('npm run build', uiDir);

rmSync(target, { recursive: true, force: true });
cpSync(dist, target, { recursive: true });

process.stdout.write(`Copied admin UI assets \u2192 ${target}\n`);
