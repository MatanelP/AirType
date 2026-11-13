import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import { checkReleaseVersion } from './check-release-version.mjs';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'airtype-release-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'src-tauri'));
  writeFileSync(join(root, 'package.json'), JSON.stringify({ version: '1.4.3' }));
  writeFileSync(join(root, 'package-lock.json'), JSON.stringify({
    version: '1.4.3', packages: { '': { version: '1.4.3' } },
  }));
  writeFileSync(join(root, 'src-tauri/tauri.conf.json'), JSON.stringify({ version: '1.4.3' }));
  writeFileSync(join(root, 'src-tauri/Cargo.toml'), '[package]\nname = "airtype"\nversion = "1.4.3"\n\n[dependencies]\n');
  writeFileSync(join(root, 'src-tauri/Cargo.lock'), 'version = 4\n\n[[package]]\nname = "airtype"\nversion = "1.4.3"\n');
  return root;
}

test('matching manifests pass for tag and manual branch builds', (t) => {
  const root = fixture(t);
  assert.equal(checkReleaseVersion(root, 'v1.4.3'), '1.4.3');
  assert.equal(checkReleaseVersion(root), '1.4.3');
});

test('a release tag cannot advertise a different binary version', (t) => {
  assert.throws(() => checkReleaseVersion(fixture(t), 'v1.4.4'), /must match app version/);
});

for (const file of ['package.json', 'package-lock.json', 'src-tauri/Cargo.toml', 'src-tauri/Cargo.lock', 'src-tauri/tauri.conf.json']) {
  test(`reject version drift in ${file}`, (t) => {
    const root = fixture(t);
    const path = join(root, file);
    writeFileSync(path, readFileSync(path, 'utf8').replace('1.4.3', '1.4.2'));
    assert.throws(() => checkReleaseVersion(root), /App versions must match/);
  });
}

const artifacts = {
  'linux-x86_64': 'AirType_1.4.3_amd64.AppImage',
  'darwin-aarch64': 'AirType.app.tar.gz',
  'windows-x86_64': 'AirType_1.4.3_x64-setup.exe',
};

function signedArtifacts(t) {
  const root = fixture(t);
  for (const name of Object.values(artifacts)) {
    writeFileSync(join(root, name), 'fixture binary');
    writeFileSync(join(root, `${name}.sig`), 'fixture signature');
  }
  return root;
}

function generate(root) {
  return spawnSync('bash', [resolve('scripts/generate-updater-manifest.sh'), root], {
    encoding: 'utf8',
    env: { ...process.env, GITHUB_REF_NAME: 'v1.4.3', GITHUB_REPOSITORY: 'MatanelP/AirType' },
  });
}

test('complete release includes exactly three signed platforms', (t) => {
  const root = signedArtifacts(t);
  const result = generate(root);
  assert.equal(result.status, 0, result.stderr);
  const manifest = JSON.parse(readFileSync(join(root, 'latest.json'), 'utf8'));
  assert.equal(manifest.version, '1.4.3');
  assert.deepEqual(Object.keys(manifest.platforms).sort(), Object.keys(artifacts).sort());
  for (const [key, name] of Object.entries(artifacts)) {
    assert.equal(manifest.platforms[key].signature, 'fixture signature');
    assert.equal(manifest.platforms[key].url, `https://github.com/MatanelP/AirType/releases/download/v1.4.3/${name}`);
  }
});

for (const [platform, name] of Object.entries(artifacts)) {
  test(`reject a release missing ${platform}`, (t) => {
    const root = signedArtifacts(t);
    rmSync(join(root, `${name}.sig`));
    const result = generate(root);
    assert.notEqual(result.status, 0);
    assert.match(result.stderr, new RegExp(`Missing signed updater artifact for ${platform}`));
  });
}

test('reject a signature without its updater binary', (t) => {
  const root = signedArtifacts(t);
  rmSync(join(root, artifacts['linux-x86_64']));
  const result = generate(root);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Missing or empty updater artifact/);
});

test('reject blank signatures', (t) => {
  const root = signedArtifacts(t);
  writeFileSync(join(root, `${artifacts['linux-x86_64']}.sig`), ' \n');
  const result = generate(root);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Empty updater signature/);
});

test('reject duplicate updater targets', (t) => {
  const root = signedArtifacts(t);
  writeFileSync(join(root, 'extra.AppImage'), 'fixture binary');
  writeFileSync(join(root, 'extra.AppImage.sig'), 'fixture signature');
  const result = generate(root);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Multiple updater artifacts/);
});
