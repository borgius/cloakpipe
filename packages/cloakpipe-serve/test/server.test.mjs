import { test } from 'node:test';
import assert from 'node:assert/strict';
import { shouldProxy, resolveStatic } from '../lib/server.mjs';

test('shouldProxy matches backend namespaces', () => {
  for (const p of ['/admin', '/admin/api/system', '/v1/pseudonymize', '/tree/x', '/sessions', '/health']) {
    assert.equal(shouldProxy(p), true, `${p} should proxy`);
  }
});

test('shouldProxy rejects SPA and static paths', () => {
  for (const p of ['/', '/profiles', '/assets/index.js', '/vault', '/administration']) {
    assert.equal(shouldProxy(p), false, `${p} should not proxy`);
  }
});

test('resolveStatic blocks path traversal', () => {
  const root = '/srv/app';
  assert.equal(resolveStatic(root, '/index.html'), '/srv/app/index.html');
  assert.equal(resolveStatic(root, '/assets/app.js'), '/srv/app/assets/app.js');
  assert.equal(resolveStatic(root, '/../../etc/passwd'), null);
  assert.equal(resolveStatic(root, '/%2e%2e/%2e%2e/etc/passwd'), null);
});
