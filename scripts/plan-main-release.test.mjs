import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
import { planMainRelease } from './plan-main-release.mjs';

const sha = 'a'.repeat(40);
const olderSha = 'b'.repeat(40);
const input = { version: '1.5.0', sha, ref: 'refs/heads/main', tagSha: null };

test('new main version creates an immutable tag and dispatches its build', () => {
  assert.deepEqual(planMainRelease(input), {
    tag: 'v1.5.0', sha, create_tag: true, dispatch: true,
  });
});

test('retry at the tagged commit dispatches without recreating the tag', () => {
  assert.deepEqual(planMainRelease({ ...input, tagSha: sha }), {
    tag: 'v1.5.0', sha, create_tag: false, dispatch: true,
  });
});

test('unchanged version at a later commit never moves or rebuilds the older tag', () => {
  assert.deepEqual(planMainRelease({ ...input, tagSha: olderSha }), {
    tag: 'v1.5.0', sha, create_tag: false, dispatch: false,
  });
});

test('only main can create or dispatch a release', () => {
  for (const ref of ['refs/heads/feature', 'refs/tags/v1.5.0', '', undefined]) {
    assert.throws(() => planMainRelease({ ...input, ref }), /main/);
  }
});

test('release tags require a stable semantic version rather than arbitrary refs', () => {
  for (const version of ['main', '1.5', '01.5.0', '1.5.0/other', '1.5.0\nref', '1.5.0-beta.1', '', undefined]) {
    assert.throws(() => planMainRelease({ ...input, version }), /version/);
  }
});

test('the release target and existing tag must resolve to full commit SHAs', () => {
  for (const invalid of ['main', 'abc123', 'a'.repeat(39), 'a'.repeat(40) + '\n', '', undefined]) {
    assert.throws(() => planMainRelease({ ...input, sha: invalid }), /commit SHA/);
    assert.throws(() => planMainRelease({ ...input, tagSha: invalid }), /commit SHA/);
  }
});

const plannerCli = fileURLToPath(new URL('./plan-main-release.mjs', import.meta.url));

function checkoutFixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'airtype-main-release-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'src-tauri'));
  const manifests = {
    'package.json': JSON.stringify({ version: '1.5.0' }),
    'package-lock.json': JSON.stringify({ version: '1.5.0', packages: { '': { version: '1.5.0' } } }),
    'src-tauri/tauri.conf.json': JSON.stringify({ version: '1.5.0' }),
    'src-tauri/Cargo.toml': '[package]\nname = "airtype"\nversion = "1.5.0"\n',
    'src-tauri/Cargo.lock': 'version = 4\n\n[[package]]\nname = "airtype"\nversion = "1.5.0"\n',
  };
  for (const [name, contents] of Object.entries(manifests)) {
    writeFileSync(join(root, name), contents);
  }
  const env = {
    ...process.env,
    GIT_AUTHOR_NAME: 'Release Test', GIT_AUTHOR_EMAIL: 'release-test@example.invalid',
    GIT_COMMITTER_NAME: 'Release Test', GIT_COMMITTER_EMAIL: 'release-test@example.invalid',
  };
  const git = (...args) => execFileSync('git', [
    '-c', 'user.name=Release Test', '-c', 'user.email=release-test@example.invalid',
    '-c', 'commit.gpgsign=false', '-c', 'tag.gpgSign=false',
    '-c', `core.hooksPath=${join(root, 'no-hooks')}`, ...args,
  ], { cwd: root, env, encoding: 'utf8', timeout: 10_000, stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  git('init', '--quiet', '--initial-branch=main');
  git('add', '--', ...Object.keys(manifests));
  git('commit', '--quiet', '-m', 'fixture manifests');
  return { root, git, env };
}

function runPlanner(fixture, expectedSha = fixture.git('rev-parse', 'HEAD')) {
  const output = join(fixture.root, 'github-output');
  const refsBefore = fixture.git('show-ref');
  const options = {
    cwd: fixture.root, encoding: 'utf8', timeout: 10_000,
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...fixture.env, GITHUB_SHA: expectedSha, GITHUB_REF: 'refs/heads/main', GITHUB_OUTPUT: output },
  };
  let result;
  try {
    result = { status: 0, stdout: execFileSync(process.execPath, [plannerCli], options), stderr: '' };
  } catch (error) {
    // Only expected CLI exits become assertion data; signals/timeouts remain failures.
    if (error.signal || error.code === 'ETIMEDOUT'
        || error.status === null || error.status === undefined) throw error;
    result = {
      status: error.status, stdout: error.stdout, stderr: error.stderr,
      error: {
        message: error.message, code: error.code ?? null, status: error.status,
        signal: error.signal ?? null, stack: error.stack,
      },
    };
  }
  assert.equal(fixture.git('show-ref'), refsBefore, 'planning must not mutate any Git ref');
  return { ...result, output };
}

function assertCliPlan(result, expected) {
  assert.equal(result.status, 0, JSON.stringify(result));
  assert.deepEqual(JSON.parse(result.stdout), expected);
  assert.equal(readFileSync(result.output, 'utf8'),
    Object.entries(expected).map(([key, value]) => `${key}=${value}\n`).join(''));
}

test('CLI validates fixture manifests and writes new-tag outputs without creating the tag', (t) => {
  const fixture = checkoutFixture(t);
  const commit = fixture.git('rev-parse', 'HEAD');
  assertCliPlan(runPlanner(fixture), {
    tag: 'v1.5.0', sha: commit, create_tag: true, dispatch: true,
  });
  assert.equal(fixture.git('tag', '--list'), '');
});

test('CLI peels an annotated tag to the triggering commit for recovery dispatch', (t) => {
  const fixture = checkoutFixture(t);
  fixture.git('tag', '--annotate', 'v1.5.0', '--message', 'fixture version');
  const commit = fixture.git('rev-parse', 'HEAD');
  assert.notEqual(fixture.git('rev-parse', 'refs/tags/v1.5.0'), commit, 'fixture tag must be annotated');
  assertCliPlan(runPlanner(fixture), {
    tag: 'v1.5.0', sha: commit, create_tag: false, dispatch: true,
  });
});

test('CLI skips the prior version tag after a later unchanged-version commit', (t) => {
  const fixture = checkoutFixture(t);
  fixture.git('tag', 'v1.5.0');
  fixture.git('commit', '--quiet', '--allow-empty', '--message', 'later unchanged version');
  assertCliPlan(runPlanner(fixture), {
    tag: 'v1.5.0', sha: fixture.git('rev-parse', 'HEAD'), create_tag: false, dispatch: false,
  });
});

test('CLI rejects a checkout that differs from the triggering SHA before writing outputs', (t) => {
  const fixture = checkoutFixture(t);
  const result = runPlanner(fixture, sha);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /Checked-out commit must match/, JSON.stringify(result));
  assert.equal(result.stdout, '');
  assert.equal(existsSync(result.output), false);
});

test('CLI rejects manifest version drift without touching an existing tag or writing outputs', (t) => {
  const fixture = checkoutFixture(t);
  fixture.git('tag', 'v1.5.0');
  writeFileSync(join(fixture.root, 'src-tauri/tauri.conf.json'), JSON.stringify({ version: '1.4.9' }));
  const result = runPlanner(fixture);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /App versions must match/, JSON.stringify(result));
  assert.equal(result.stdout, '');
  assert.equal(existsSync(result.output), false);
});
