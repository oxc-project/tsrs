import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';
import { needsRelease, publishPackages, registryVersion, validateArtifacts } from '../scripts/release.mjs';
import { targets } from '../scripts/targets.mjs';

const pkg = JSON.parse(fs.readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
const response = (version, status = 200) => async () => ({ status, ok: status === 200, json: async () => ({ name: pkg.name, version }) });

test('release detection handles first, unchanged, newer and invalid versions', async () => {
  assert.equal(await needsRelease(pkg, response(null, 404)), true);
  assert.equal(await needsRelease(pkg, response(pkg.version)), false);
  assert.equal(await needsRelease({ ...pkg, version: '1.0.0' }, response(pkg.version)), true);
  await assert.rejects(needsRelease(pkg, response('9.0.0')), /older/);
  await assert.rejects(needsRelease({ ...pkg, version: '1.0.0-beta.1' }, response(null, 404)), /stable semver/);
  for (const status of [401, 403, 429, 500]) {
    await assert.rejects(needsRelease(pkg, response(null, status)), /Registry request failed/);
  }
  await assert.rejects(registryVersion(pkg.name, 'latest', response('invalid')), /Invalid registry/);
  await assert.rejects(needsRelease(pkg, () => { throw new Error('network failed'); }), /network failed/);
});

test('assembly requires every configured binary, exactly once and nonempty', t => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'tsrs-napi-artifacts-'));
  t.after(() => fs.rmSync(dir, { recursive: true, force: true }));
  const files = targets.map(({ suffix }) => {
    const file = path.join(dir, `tsrs.${suffix}.node`);
    fs.writeFileSync(file, 'test binary');
    return file;
  });
  validateArtifacts(pkg, files);
  assert.throws(() => validateArtifacts(pkg, files.slice(1)), /exactly one nonempty/);
  assert.throws(() => validateArtifacts(pkg, [...files, files[0]]), /exactly one nonempty/);
  fs.truncateSync(files[0]);
  assert.throws(() => validateArtifacts(pkg, files), /exactly one nonempty/);
  assert.throws(() => validateArtifacts({ ...pkg, napi: { ...pkg.napi, targets: [] } }, files), /target table/);
});

test('retry skips existing versions and waits for platforms before publishing root', async () => {
  const events = [];
  const release = { version: pkg.version, packages: [{ name: 'platform-a' }, { name: 'platform-b' }, { name: pkg.name }] };
  await publishPackages(release, {
    view: async name => name === 'platform-a' ? pkg.version : null,
    publish: async p => { events.push(`publish ${p.name}`); },
    wait: async p => { events.push(`wait ${p.name}`); },
  });
  assert.deepEqual(events, ['wait platform-a', 'publish platform-b', 'wait platform-b', `publish ${pkg.name}`, `wait ${pkg.name}`]);
  await assert.rejects(publishPackages(release, {
    view: async () => null,
    publish: async () => {},
    wait: async () => { throw new Error('platform unavailable'); },
  }), /platform unavailable/);
});
