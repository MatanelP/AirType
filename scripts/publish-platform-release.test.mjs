import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { publishPlatform, packageNames, githubApi } from './publish-platform-release.mjs';

const repo = 'MatanelP/AirType';
const tag = 'v1.10.0';
const targets = ['linux-x86_64', 'darwin-aarch64', 'windows-x86_64'];
// Tauri .sig files contain a base64-encoded minisign signature, not arbitrary text.
const signature = Buffer.from('untrusted comment: signature from minisign secret key\n'
  + Buffer.concat([Buffer.from('ED'), Buffer.alloc(72, 1)]).toString('base64') + '\ntrusted comment: timestamp:1\n'
  + Buffer.alloc(64, 2).toString('base64') + '\n').toString('base64');
const sha = (data) => `sha256:${createHash('sha256').update(data).digest('hex')}`;

function fixture(t) {
  const directory = mkdtempSync(join(tmpdir(), 'airtype-progressive-release-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  const releases = new Map();
  const events = [];
  let latest = null;
  let failure;
  function event(kind, name) {
    events.push([kind, name]);
    if (failure?.(kind, name)) throw new Error(`injected ${kind} failure: ${name}`);
  }
  const api = {
    release(releaseTag) { event('read', releaseTag); return releases.get(releaseTag) ?? null; },
    latest() { event('latest'); return latest; },
    create(releaseTag) {
      event('create', releaseTag);
      releases.set(releaseTag, { tag_name: releaseTag, draft: true, prerelease: false,
        immutable: false, created_at: '2026-11-13T00:00:00Z', assets: [] });
    },
    signature(asset) { event('signature', asset.name); return asset.content; },
    remove(asset) {
      event('remove', asset.name);
      for (const release of releases.values()) release.assets = release.assets.filter((item) => item !== asset);
    },
    upload(releaseTag, file, replace = false) {
      const name = file.split('/').at(-1);
      event('upload', name);
      const release = releases.get(releaseTag);
      if (replace) release.assets = release.assets.filter((asset) => asset.name !== name);
      if (release.assets.some((asset) => asset.name === name)) throw new Error('duplicate asset');
      const content = readFileSync(file);
      release.assets.push({ id: release.assets.length + 1, name, content: content.toString(),
        size: content.length, state: 'uploaded', digest: sha(content) });
    },
    publish(releaseTag) { event('publish', releaseTag); releases.get(releaseTag).draft = false; },
    promote(releaseTag) { event('promote', releaseTag); latest = releases.get(releaseTag); },
  };
  function run(platform, options = {}) {
    const releaseTag = options.tag ?? tag;
    const dir = join(directory, platform);
    mkdirSync(dir, { recursive: true });
    for (const name of packageNames(releaseTag, platform)) {
      writeFileSync(join(dir, name), name.endsWith('.sig') ? signature : `binary ${name}`);
    }
    options.change?.(dir);
    return publishPlatform({ repo, tag: releaseTag, platform, directory: dir, api });
  }
  const manifest = (releaseTag = tag) => JSON.parse(releases.get(releaseTag).assets
    .find((asset) => asset.name === 'latest.json').content);
  return { api, run, releases, events, manifest, fail: (predicate) => { failure = predicate; },
    latest: () => latest, setLatest: (value) => { latest = value; } };
}

for (const order of [targets, [targets[0], targets[2], targets[1]],
  [targets[1], targets[0], targets[2]], [targets[1], targets[2], targets[0]],
  [targets[2], targets[0], targets[1]], [targets[2], targets[1], targets[0]]]) {
  test(`one release grows monotonically in arrival order ${order.join(', ')}`, (t) => {
    const f = fixture(t);
    for (const [index, platform] of order.entries()) {
      const result = f.run(platform);
      assert.deepEqual(Object.keys(result.platforms).sort(), order.slice(0, index + 1).sort());
      assert.equal(f.manifest().version, '1.10.0');
      assert.equal(f.manifest().pub_date, '2026-11-13T00:00:00Z');
      assert.equal(f.latest().tag_name, tag);
      assert.equal(f.latest().draft, false);
      for (const value of Object.values(result.platforms)) {
        assert.equal(value.signature, signature);
        assert.ok(value.url.startsWith(`https://github.com/${repo}/releases/download/${tag}/`));
      }
    }
    assert.equal(f.releases.size, 1);
    const firstManifest = f.events.findIndex(([kind, name]) => kind === 'upload' && name === 'latest.json');
    assert.ok(firstManifest < f.events.findIndex(([kind]) => kind === 'publish'));
    assert.equal(f.events.filter(([kind]) => kind === 'create').length, 1);
  });
}

test('identical retries preserve packages, targets and publication date', (t) => {
  const f = fixture(t);
  f.run(targets[0]); f.run(targets[1]);
  const before = f.manifest();
  f.events.length = 0;
  assert.deepEqual(f.run(targets[0]), before);
  assert.deepEqual(f.events.filter(([kind]) => kind === 'upload'), [['upload', 'latest.json']]);
});

for (const platform of targets) {
  for (const name of packageNames(tag, platform)) {
    test(`retry recovers ${platform} upload failure at ${name}`, (t) => {
      const f = fixture(t);
      f.fail((kind, asset) => kind === 'upload' && asset === name);
      assert.throws(() => f.run(platform), /injected upload failure/);
      assert.equal(f.latest(), null);
      f.fail();
      f.run(platform);
      assert.deepEqual(Object.keys(f.manifest().platforms), [platform]);
    });
  }
}

test('deleted manifest is rebuilt from all existing signed package sets', (t) => {
  const f = fixture(t);
  f.run(targets[0]); f.run(targets[1]);
  f.releases.get(tag).assets = f.releases.get(tag).assets.filter((a) => a.name !== 'latest.json');
  f.run(targets[2]);
  assert.deepEqual(Object.keys(f.manifest().platforms).sort(), [...targets].sort());
});

test('manifest replacement failure recovers without losing published platforms', (t) => {
  const f = fixture(t);
  f.run(targets[0]);
  f.fail((kind, name) => {
    if (kind !== 'upload' || name !== 'latest.json') return false;
    f.releases.get(tag).assets = f.releases.get(tag).assets.filter((a) => a.name !== name);
    return true;
  });
  assert.throws(() => f.run(targets[1]), /injected/);
  f.fail(); f.run(targets[2]);
  assert.deepEqual(Object.keys(f.manifest().platforms).sort(), [...targets].sort());
});

for (const name of [...packageNames(tag, targets[0]), 'latest.json']) {
  test(`retry tolerates server accepting ${name} before the upload response fails`, (t) => {
    const f = fixture(t);
    const original = f.api.upload;
    let failed = false;
    f.api.upload = (releaseTag, path, replace) => {
      original(releaseTag, path, replace);
      if (!failed && path.endsWith(`/${name}`)) { failed = true; throw new Error('response lost'); }
    };
    assert.throws(() => f.run(targets[0]), /response lost/);
    f.run(targets[0]);
    assert.equal(f.latest().tag_name, tag);
    assert.deepEqual(Object.keys(f.manifest().platforms), [targets[0]]);
  });
}

test('a changed signature cannot replace one already advertised', (t) => {
  const f = fixture(t); f.run(targets[0]);
  assert.throws(() => f.run(targets[0], { change(dir) {
    writeFileSync(join(dir, packageNames(tag, targets[0]).at(-1)), `${signature}\n`);
  } }), /conflicting.*asset/i);
});

test('unexpected local target/version is rejected before any API call', (t) => {
  const f = fixture(t);
  assert.throws(() => f.run(targets[0], { change(dir) {
    writeFileSync(join(dir, 'AirType_1.9.0_amd64.AppImage'), 'wrong build');
  } }), /unexpected/i);
  assert.equal(f.events.length, 0);
});

test('changed package bytes cannot replace an advertised signed package', (t) => {
  const f = fixture(t); f.run(targets[0]);
  f.events.length = 0;
  assert.throws(() => f.run(targets[0], { change(dir) {
    writeFileSync(join(dir, packageNames(tag, targets[0])[0]), 'different build');
  } }), /conflicting.*asset/i);
  assert.equal(f.events.some(([kind]) => kind === 'upload'), false);
});

test('unknown remote digest fails closed without uploading', (t) => {
  const f = fixture(t); f.run(targets[0]);
  delete f.releases.get(tag).assets[0].digest;
  f.events.length = 0;
  assert.throws(() => f.run(targets[0]), /digest/i);
  assert.equal(f.events.some(([kind]) => kind === 'upload'), false);
});

for (const [field, value, error] of [['immutable', true, /immutable/i],
  ['prerelease', true, /prerelease/i], ['tag_name', 'v1.9.0', /tag/i]]) {
  test(`reject unsafe existing release metadata ${field}`, (t) => {
    const f = fixture(t); f.run(targets[0]);
    f.releases.get(tag)[field] = value;
    f.events.length = 0;
    assert.throws(() => f.run(targets[1]), error);
    assert.equal(f.events.some(([kind]) => kind === 'upload'), false);
  });
}

test('old-tag completion never demotes 1.10.0 to 1.9.0', (t) => {
  const f = fixture(t); f.run(targets[0]);
  f.run(targets[1], { tag: 'v1.9.0' });
  assert.equal(f.latest().tag_name, tag);
  assert.deepEqual(Object.keys(f.manifest('v1.9.0').platforms), [targets[1]]);
});

test('numeric comparison promotes 1.10.0 over 1.9.0', (t) => {
  const f = fixture(t); f.run(targets[0], { tag: 'v1.9.0' });
  f.run(targets[1]);
  assert.equal(f.latest().tag_name, tag);
});

for (const badTag of ['vnext', 'v1.10.0-beta.1', '1.10.0', 'v01.2.3']) {
  test(`malformed latest version ${badTag} is never treated as absent`, (t) => {
    const f = fixture(t); f.setLatest({ tag_name: badTag });
    assert.throws(() => f.run(targets[0]), /stable.*version/i);
    assert.equal(f.events.some(([kind]) => kind === 'create'), false);
  });
}

test('API failure is an error, never an empty release', (t) => {
  const f = fixture(t); f.fail((kind) => kind === 'read');
  assert.throws(() => f.run(targets[0]), /injected read failure/);
  assert.equal(f.releases.size, 0);
});

for (const platform of targets) {
  test(`missing required installer prevents ${platform} publication`, (t) => {
    const f = fixture(t);
    assert.throws(() => f.run(platform, { change(dir) {
      rmSync(join(dir, packageNames(tag, platform)[0]));
    } }), /required.*package/i);
    assert.equal(f.releases.size, 0);
  });
}

test('malformed local signature fails before creating a draft', (t) => {
  const f = fixture(t);
  assert.throws(() => f.run(targets[0], { change(dir) {
    writeFileSync(join(dir, packageNames(tag, targets[0]).at(-1)), 'not a signature');
  } }), /signature/i);
  assert.equal(f.releases.size, 0);
});

test('signature download failure prevents advertising another platform', (t) => {
  const f = fixture(t); f.run(targets[0]);
  const before = f.manifest();
  f.fail((kind) => kind === 'signature');
  assert.throws(() => f.run(targets[1]), /injected signature failure/);
  assert.deepEqual(f.manifest(), before);
});

test('incomplete previous target stays omitted until its signature is uploaded', (t) => {
  const f = fixture(t);
  const signatureName = packageNames(tag, targets[0]).at(-1);
  f.fail((kind, name) => kind === 'upload' && name === signatureName);
  assert.throws(() => f.run(targets[0]), /injected/);
  f.fail(); f.run(targets[1]);
  assert.deepEqual(Object.keys(f.manifest().platforms), [targets[1]]);
  f.run(targets[0]);
  assert.deepEqual(Object.keys(f.manifest().platforms).sort(), targets.slice(0, 2).sort());
});

for (const name of [packageNames(tag, targets[0])[0], packageNames(tag, targets[0]).at(-1)]) {
  test(`retry recovers GitHub's empty starter asset ${name}`, (t) => {
    const f = fixture(t);
    f.fail((kind, asset) => {
      if (kind !== 'upload' || asset !== name) return false;
      f.releases.get(tag).assets.push({ id: 999, name, state: 'starter', size: 0, digest: null });
      return true;
    });
    assert.throws(() => f.run(targets[0]), /injected/);
    f.fail(); f.run(targets[0]);
    assert.deepEqual(Object.keys(f.manifest().platforms), [targets[0]]);
    assert.deepEqual(f.events.filter(([kind]) => kind === 'remove'), [['remove', name]]);
  });
}

test('one platform with an unfinished starter signature cannot block another platform', (t) => {
  const f = fixture(t);
  const name = packageNames(tag, targets[0]).at(-1);
  f.fail((kind, asset) => {
    if (kind !== 'upload' || asset !== name) return false;
    f.releases.get(tag).assets.push({ id: 999, name, state: 'starter', size: 0, digest: null });
    return true;
  });
  assert.throws(() => f.run(targets[0]), /injected/);
  f.fail(); f.run(targets[1]);
  assert.deepEqual(Object.keys(f.manifest().platforms), [targets[1]]);
  assert.equal(f.events.some(([kind]) => kind === 'remove'), false, 'only the current target may remove its starter');
  f.run(targets[0]);
  assert.deepEqual(Object.keys(f.manifest().platforms).sort(), targets.slice(0, 2).sort());
});

for (const mutation of [
  (assets) => { assets.find((a) => a.name.endsWith('.sig')).content = 'invalid'; },
  (assets) => { assets.find((a) => a.name.endsWith('.sig')).state = 'new'; },
  (assets) => { assets.find((a) => a.name.endsWith('.AppImage')).size = 0; },
  (assets) => { assets.push({ ...assets.find((a) => a.name.endsWith('.sig')), name: 'extra.AppImage.sig' }); },
]) {
  test('invalid existing signed sets cannot produce a manifest', (t) => {
    const f = fixture(t); f.run(targets[0]);
    mutation(f.releases.get(tag).assets);
    assert.throws(() => f.run(targets[1]), /signature|uploaded|package|unexpected/i);
  });
}

test('GitHub adapter paginates fresh asset IDs and keeps packages append-only', () => {
  const calls = [];
  const api = githubApi(repo, (args) => {
    calls.push(args);
    if (args.includes('--paginate')) return '{"id":1}\n{"id":2}\n';
    if (args[1].endsWith('/tags/v1.10.0')) return '{"id":42,"assets":[]}';
    return '{}';
  });
  assert.deepEqual(api.release(tag).assets, [{ id: 1 }, { id: 2 }]);
  assert.ok(calls[2].includes(`repos/${repo}/releases/42/assets?per_page=100`));
  assert.ok(calls[2].includes('--paginate'));
  api.upload(tag, '/artifact/setup.exe');
  assert.equal(calls.at(-1).includes('--clobber'), false);
  api.upload(tag, '/artifact/latest.json', true);
  assert.equal(calls.at(-1).includes('--clobber'), true);
  api.create(tag); assert.ok(calls.at(-1).includes('--verify-tag'));
  assert.ok(calls.at(-1).includes('--draft'));
  assert.ok(calls.at(-1).includes('--latest=false'));
  api.publish(tag); assert.ok(calls.at(-1).includes('--latest=false'));
  api.signature({ id: 99 });
  assert.ok(calls.at(-1).includes(`repos/${repo}/releases/assets/99`));
});

for (const status of [401, 403, 422, 429, 500]) {
  test(`GitHub HTTP ${status} is not mistaken for an absent release`, () => {
    const api = githubApi(repo, () => { throw Object.assign(new Error('API failed'), {
      status: 1, stderr: `gh: failure (HTTP ${status})`,
    }); });
    assert.throws(() => api.release(tag), /API failed/);
    assert.throws(() => api.latest(), /API failed/);
  });
}

test('GitHub HTTP 404 is the only absent-release response', () => {
  const api = githubApi(repo, (args) => {
    if (args.includes('--paginate')) return '';
    throw Object.assign(new Error('not found'), {
    status: 1, stderr: 'gh: Not Found (HTTP 404)',
  }); });
  assert.equal(api.release(tag), null);
  assert.equal(api.latest(), null);
});

test('adapter resolves an existing/newly-created draft when the published-tag lookup returns404', () => {
  const api = githubApi(repo, (args) => {
    if (args.includes(`repos/${repo}/releases/tags/${tag}`)) {
      throw Object.assign(new Error('published tag absent'), { status: 1, stderr: 'gh: Not Found (HTTP 404)' });
    }
    if (args.includes(`repos/${repo}/releases?per_page=100`)) return `{"id":42,"tag_name":"${tag}"}\n`;
    if (args.includes(`repos/${repo}/releases/42`)) return `{"id":42,"tag_name":"${tag}","draft":true}`;
    if (args.includes(`repos/${repo}/releases/42/assets?per_page=100`)) return '';
    return '{}';
  });
  assert.equal(api.release(tag)?.draft, true);
  api.create(tag);
  assert.equal(api.release(tag)?.id, 42);
});

test('draft fallback never treats a release-list API error as absence', () => {
  const api = githubApi(repo, (args) => {
    if (args.includes('--paginate')) throw new Error('release list forbidden');
    throw Object.assign(new Error('not found'), { status: 1, stderr: 'gh: Not Found (HTTP 404)' });
  });
  assert.throws(() => api.release(tag), /release list forbidden/);
});

test('malformed GitHub JSON fails closed', () => {
  const api = githubApi(repo, () => '<html>temporary failure</html>');
  assert.throws(() => api.release(tag), SyntaxError);
});

for (const immutable of [true, undefined]) {
  test(`first publication becoming immutable=${immutable} stops before promotion`, (t) => {
    const f = fixture(t);
    f.api.publish = (releaseTag) => {
      f.releases.get(releaseTag).draft = false;
      f.releases.get(releaseTag).immutable = immutable;
    };
    assert.throws(() => f.run(targets[0]), /immutable/i);
    assert.equal(f.latest(), null);
  });
}

test('failure publishing a draft can resume from the already uploaded packages', (t) => {
  const f = fixture(t);
  f.fail((kind) => kind === 'publish');
  assert.throws(() => f.run(targets[0]), /injected publish failure/);
  assert.equal(f.releases.get(tag).draft, true);
  f.fail(); f.run(targets[0]);
  assert.equal(f.latest().tag_name, tag);
});

test('promotion failure can resume without changing packages or release notes', (t) => {
  const f = fixture(t);
  f.fail((kind) => kind === 'promote');
  assert.throws(() => f.run(targets[0]), /injected promote failure/);
  assert.equal(f.releases.get(tag).draft, false);
  f.fail(); f.run(targets[0]);
  assert.equal(f.latest().tag_name, tag);
});

test('branch builds cannot publish and each publisher waits only for its platform', () => {
  const workflow = readFileSync(new URL('../.github/workflows/build.yml', import.meta.url), 'utf8');
  for (const platform of ['linux', 'macos', 'windows']) {
    const block = workflow.split(`  publish-${platform}:\n`)[1]?.split(/\n  [a-z]/)[0];
    assert.ok(block, `missing ${platform} publisher`);
    assert.match(block, /if: startsWith\(github.ref, 'refs\/tags\/v'\)/);
    assert.match(block, new RegExp(`needs: build-${platform}\\n`));
    assert.match(block, /uses: \.\/\.github\/workflows\/publish-platform.yml/);
    assert.match(block, /artifact-name: .*\$\{\{ github.sha \}\}-\$\{\{ github.run_id \}\}/);
  }
  const publisher = readFileSync(new URL('../.github/workflows/publish-platform.yml', import.meta.url), 'utf8');
  assert.match(publisher, /group: airtype-release-publication/);
  assert.match(publisher, /cancel-in-progress: false/);
  assert.match(publisher, /queue: max/);
  assert.equal(workflow.includes('concurrency:'), false, 'avoid nested lock deadlocks');
});
