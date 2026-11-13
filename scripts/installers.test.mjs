// Only local API fixtures are read. The curl double always fails downloads,
// so these tests cannot install or execute a downloaded application.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, existsSync, mkdtempSync, mkdirSync, readFileSync,
  rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

const scripts = dirname(fileURLToPath(import.meta.url));
const repo = 'MatanelP/AirType';
const asset = (tag, type = 'deb') => {
  const version = tag.replace(/^v/, '');
  return ({ deb: `AirType_${version}_amd64.deb`, rpm: `AirType-${version}-1.x86_64.rpm`,
    appimage: `AirType_${version}_amd64.AppImage`, dmg: `AirType_${version}_aarch64.dmg` })[type];
};
const release = (tag, type = 'deb', extra = {}) => ({ tag_name: tag, draft: false,
  prerelease: false, assets: [{ name: asset(tag, type), state: 'uploaded' }], ...extra });

function fixture(t, { latest, releases, type = 'deb', pin } = {}) {
  const directory = mkdtempSync(join(tmpdir(), 'airtype-installer-test-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const bin = join(directory, 'bin');
  mkdirSync(bin);
  for (const name of ['awk', 'grep', 'head', 'sed', 'mktemp', 'rm', 'cat']) {
    const source = process.env.PATH.split(':').map((entry) => join(entry, name)).find(existsSync);
    assert.ok(source, `Test prerequisite ${name}`);
    symlinkSync(source, join(bin, name));
  }
  for (const name of ['curl', 'uname']) {
    copyFileSync(join(scripts, 'test-fixtures', `installer-${name}.sh`), join(bin, name));
    chmodSync(join(bin, name), 0o755);
  }
  if (type === 'deb') symlinkSync('/bin/false', join(bin, 'dpkg'));
  if (type === 'rpm') symlinkSync('/bin/false', join(bin, 'rpm'));
  if (latest !== undefined) writeFileSync(join(directory, 'latest.json'),
    typeof latest === 'string' ? latest : JSON.stringify(latest));
  if (releases !== undefined) writeFileSync(join(directory, 'releases.json'), JSON.stringify(releases));
  const log = join(directory, 'curl.log');
  const result = spawnSync('/bin/sh', [join(scripts, 'install.sh')], {
    env: { ...process.env, PATH: bin, AIRTYPE_VERSION: pin ?? '', AIRTYPE_REPO: repo,
      AIRTYPE_NO_LAUNCH: '1', AIRTYPE_INSTALLER_FIXTURES: directory,
      AIRTYPE_INSTALLER_CURL_LOG: log, AIRTYPE_MOCK_OS: type === 'dmg' ? 'Darwin' : 'Linux',
      AIRTYPE_MOCK_ARCH: type === 'dmg' ? 'arm64' : 'x86_64' },
    encoding: 'utf8', timeout: 10000,
  });
  assert.equal(result.signal, null, 'Installer test must finish without a signal');
  assert.equal(result.status, 1, 'Mock downloads deliberately fail before installation');
  const urls = existsSync(log) ? readFileSync(log, 'utf8').trim().split('\n') : [];
  return { ...result, urls, downloads: urls.filter((url) => url.includes('/releases/download/')) };
}

for (const type of ['deb', 'rpm', 'appimage', 'dmg']) {
  test(`unpinned ${type} uses latest directly when its installer is ready`, (t) => {
    const f = fixture(t, { type, latest: release('v1.10.0', type) });
    assert.deepEqual(f.downloads, [`https://github.com/${repo}/releases/download/v1.10.0/${asset('v1.10.0', type)}`]);
    assert.equal(f.urls.length, 2);
  });

  test(`unpinned ${type} falls back to the newest compatible stable version`, (t) => {
    const f = fixture(t, { type, latest: release('v1.11.0', type, { assets: [] }), releases: [
      release('v1.9.0', type), release('v1.10.0', type), release('v1.12.0', type),
      release('v1.11.0', type, { draft: true }), release('v1.11.0', type, { prerelease: true }),
    ] });
    assert.deepEqual(f.downloads, [`https://github.com/${repo}/releases/download/v1.10.0/${asset('v1.10.0', type)}`]);
    assert.ok(f.urls.some((url) => url.endsWith('/releases?per_page=100&page=1')));
  });

  test(`explicit ${type} pin never falls back or queries latest`, (t) => {
    const f = fixture(t, { type, pin: 'v1.10.0', latest: release('v1.9.0', type) });
    assert.deepEqual(f.urls, [`https://github.com/${repo}/releases/download/v1.10.0/${asset('v1.10.0', type)}`]);
  });
}

test('latest API failure never triggers an older install', (t) => {
  const f = fixture(t, { releases: [release('v1.9.0')] });
  assert.equal(f.urls.length, 1);
  assert.deepEqual(f.downloads, []);
});

test('fallback API failure never triggers an install', (t) => {
  const f = fixture(t, { latest: release('v1.10.0', 'deb', { assets: [] }) });
  assert.equal(f.urls.length, 2);
  assert.deepEqual(f.downloads, []);
});

test('latest download failure never triggers fallback', (t) => {
  const f = fixture(t, { latest: release('v1.10.0'), releases: [release('v1.9.0')] });
  assert.equal(f.urls.length, 2);
  assert.ok(!f.urls.some((url) => url.includes('?per_page')));
});

test('missing installer fails explicitly after the bounded release scan', (t) => {
  const f = fixture(t, { latest: release('v1.10.0', 'dmg'), releases: [release('v1.9.0', 'dmg')] });
  assert.deepEqual(f.downloads, []);
  assert.match(f.stderr, /100 releases/);
});

test('escaped release body and reordered fields cannot masquerade as metadata', (t) => {
  const body = 'A quoted example: "tag_name":"v9.9.9", "draft":false and brackets [}\nA new line and \\ backslash';
  const candidate = { body, assets: release('v1.9.0').assets, prerelease: false, draft: false, tag_name: 'v1.9.0' };
  const f = fixture(t, { latest: release('v1.10.0', 'dmg', { body }), releases: [candidate] });
  assert.equal(f.downloads.length, 1);
  assert.ok(f.downloads[0].includes('/v1.9.0/'));
});

for (const latest of ['{"tag_name":"v1.10.0"', '{"tag_name":"v1.10.0","assets":[]}']) {
  test(`malformed or incomplete metadata fails closed: ${latest}`, (t) => {
    const f = fixture(t, { latest, releases: [release('v1.9.0')] });
    assert.deepEqual(f.downloads, []);
    assert.equal(f.urls.length, 1);
  });
}

test('pending uploads and nonstable tags are not compatible releases', (t) => {
  const f = fixture(t, { latest: release('v1.10.0', 'deb', { assets: [] }), releases: [
    release('v1.9.0', 'deb', { assets: [{ name: asset('v1.9.0'), state: 'new' }] }),
    release('v1.8.0-beta.1'), release('v01.7.0'),
  ] });
  assert.deepEqual(f.downloads, []);
});

test('Unicode-escaped ASCII metadata keys and asset values are decoded', (t) => {
  const latest = JSON.stringify(release('v1.10.0'))
    .replace('"tag_name"', '"t\\u0061g_name"')
    .replace('"draft"', '"dr\\u0061ft"')
    .replace('"v1.10.0"', '"v1.\\u00310.0"')
    .replace('"name"', '"n\\u0061me"')
    .replace('"AirType_', '"\\u0041irType_');
  const f = fixture(t, { latest });
  assert.deepEqual(f.downloads, [`https://github.com/${repo}/releases/download/v1.10.0/${asset('v1.10.0')}`]);
  assert.equal(f.urls.length, 2);
});

for (const latest of ['[]', '{}', '{"tag_name":"v1.10.0","draft":false,"draft":true,"prerelease":false,"assets":[]}']) {
  test(`invalid latest shape/duplicate fields never select an older release: ${latest}`, (t) => {
    const f = fixture(t, { latest, releases: [release('v1.9.0')] });
    assert.deepEqual(f.downloads, []);
    assert.equal(f.urls.length, 1);
  });
}

for (const releases of [[], {}]) {
  test(`empty or wrong-shaped fallback metadata cannot select a release: ${JSON.stringify(releases)}`, (t) => {
    const f = fixture(t, { latest: release('v1.10.0', 'deb', { assets: [] }), releases });
    assert.deepEqual(f.downloads, []);
    assert.equal(f.urls.length, 2);
  });
}

const powershell = ['pwsh', 'powershell.exe'].flatMap((name) => process.env.PATH.split(':')
  .map((entry) => join(entry, name))).find(existsSync);
test('Windows installer resolver with local API doubles', { skip: !powershell }, () => {
  const result = spawnSync(powershell, ['-NoProfile', '-File', join(scripts, 'installers-windows.test.ps1')], {
    encoding: 'utf8', timeout: 10000,
  });
  assert.equal(result.signal, null);
  assert.equal(result.status, 0, result.stderr || result.stdout);
});
