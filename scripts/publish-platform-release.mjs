import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';
import { parseArgs } from 'node:util';
import { pathToFileURL } from 'node:url';
import { checkReleaseVersion } from './check-release-version.mjs';

const targets = ['linux-x86_64', 'darwin-aarch64', 'windows-x86_64'];

function version(tag) {
  if (!/^v(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(tag)) {
    throw new Error(`Expected a stable release version, got ${tag}`);
  }
  return tag.slice(1).split('.').map(BigInt);
}

function newerOrEqual(a, b) {
  const left = version(a), right = version(b);
  for (let i = 0; i < 3; i++) {
    if (left[i] !== right[i]) return left[i] > right[i];
  }
  return true;
}

// Order matters: installers and updater binary first, commit the signed set last.
export function packageNames(tag, platform) {
  version(tag);
  const v = tag.slice(1);
  switch (platform) {
    case 'linux-x86_64': return [`AirType_${v}_amd64.deb`, `AirType-${v}-1.x86_64.rpm`,
      `AirType_${v}_amd64.AppImage`, `AirType_${v}_amd64.AppImage.sig`];
    case 'darwin-aarch64': return [`AirType_${v}_aarch64.dmg`, 'AirType.app.tar.gz', 'AirType.app.tar.gz.sig'];
    case 'windows-x86_64': return [`AirType_${v}_x64_en-US.msi`, `AirType_${v}_x64-setup.exe`,
      `AirType_${v}_x64-setup.exe.sig`];
    default: throw new Error(`Unsupported release platform: ${platform}`);
  }
}

function signature(value) {
  const text = value.trim();
  const decoded = Buffer.from(text, 'base64');
  const lines = decoded.toString('utf8').trimEnd().split('\n');
  const base64 = (line, size) => typeof line === 'string'
    && Buffer.from(line, 'base64').length === size
    && Buffer.from(line, 'base64').toString('base64') === line;
  if (decoded.toString('base64') !== text || lines.length !== 4
    || !lines[0].startsWith('untrusted comment: ') || !lines[2].startsWith('trusted comment: ')
    || !base64(lines[1], 74) || !base64(lines[3], 64)
    || !['Ed', 'ED'].includes(Buffer.from(lines[1] ?? '', 'base64').subarray(0, 2).toString())) {
    throw new Error('Malformed Tauri updater signature');
  }
  return text;
}

function filesIn(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return filesIn(path);
    if (!entry.isFile()) throw new Error(`Unexpected artifact entry: ${path}`);
    return [path];
  });
}

function validateRelease(release, tag) {
  if (!release || typeof release !== 'object') throw new Error('Invalid release metadata');
  if (release.tag_name !== tag) throw new Error('Release tag does not match requested tag');
  if (release.immutable !== false) throw new Error('Release is immutable or immutability is unknown; cannot publish progressively');
  if (release.prerelease !== false || typeof release.draft !== 'boolean') throw new Error('Invalid draft/prerelease metadata');
  if (!Array.isArray(release.assets) || !Number.isFinite(Date.parse(release.created_at))) {
    throw new Error('Invalid release assets or publication date');
  }
  const names = new Set();
  for (const asset of release.assets) {
    if (!asset || typeof asset.name !== 'string') throw new Error('Invalid release asset metadata');
    if (names.has(asset.name)) throw new Error(`Duplicate release asset: ${asset.name}`);
    names.add(asset.name);
  }
}

function uploaded(asset) {
  if (asset.state !== 'uploaded' || !Number.isSafeInteger(asset.size) || asset.size <= 0) {
    throw new Error(`Required package is not fully uploaded: ${asset.name}`);
  }
}

function emptyStarter(asset) {
  return asset?.state === 'starter' && asset.size === 0 && asset.digest === null;
}

function manifestFromAssets({ api, release, tag, repo }) {
  const assets = new Map(release.assets.map((asset) => [asset.name, asset]));
  const allowedSignatures = targets.map((target) => packageNames(tag, target).at(-1));
  for (const asset of release.assets) {
    if (asset.name.endsWith('.sig') && !allowedSignatures.includes(asset.name)) {
      throw new Error(`Unexpected updater signature/package version: ${asset.name}`);
    }
  }
  const platforms = {};
  for (const target of targets) {
    const names = packageNames(tag, target);
    // A missing signature is an unfinished attempt, never an advertised target.
    if (!assets.has(names.at(-1)) || emptyStarter(assets.get(names.at(-1)))) continue;
    for (const name of names) {
      if (!assets.has(name)) throw new Error(`Missing required package ${name} for signed target`);
      uploaded(assets.get(name));
    }
    const signatureAsset = assets.get(names.at(-1));
    const content = api.signature(signatureAsset);
    const digest = `sha256:${createHash('sha256').update(content).digest('hex')}`;
    if (signatureAsset.digest !== digest) throw new Error(`Signature download digest mismatch: ${signatureAsset.name}`);
    const sig = signature(content);
    platforms[target] = { signature: sig,
      url: `https://github.com/${repo}/releases/download/${tag}/${names.at(-2)}` };
  }
  return { version: tag.slice(1), pub_date: release.created_at, platforms };
}

// Caller MUST hold the repository-wide publisher concurrency lock throughout.
// Packages are append-only. latest.json is disposable, reconstructed metadata.
export function publishPlatform({ repo, tag, platform, directory, api }) {
  if (!/^[\w.-]+\/[\w.-]+$/.test(repo)) throw new Error('Invalid repository');
  const names = packageNames(tag, platform);
  const files = filesIn(directory);
  const paths = new Map();
  for (const file of files) {
    const name = basename(file);
    if (!names.includes(name) || paths.has(name)) throw new Error(`Unexpected or duplicate artifact: ${name}`);
    paths.set(name, file);
  }
  for (const name of names) {
    if (!paths.has(name) || statSync(paths.get(name)).size <= 0) throw new Error(`Missing required package: ${name}`);
  }
  signature(readFileSync(paths.get(names.at(-1)), 'utf8'));

  // Fail closed on unknown version/API state before making any mutation.
  const latest = api.latest();
  if (latest) version(latest.tag_name);
  let release = api.release(tag);
  if (!release) { api.create(tag); release = api.release(tag); }
  validateRelease(release, tag);

  // Preflight every collision before uploading anything. Never clobber packages.
  const missing = [];
  const starters = [];
  for (const name of names) {
    const asset = release.assets.find((item) => item.name === name);
    if (!asset) { missing.push(name); continue; }
    if (emptyStarter(asset)) {
      if (!Number.isSafeInteger(asset.id) || asset.id <= 0) throw new Error('Invalid starter asset ID');
      starters.push(asset); missing.push(name); continue;
    }
    uploaded(asset);
    if (!/^sha256:[a-f0-9]{64}$/.test(asset.digest ?? '')) throw new Error(`Missing/invalid asset digest: ${name}`);
    const digest = `sha256:${createHash('sha256').update(readFileSync(paths.get(name))).digest('hex')}`;
    if (asset.digest !== digest) throw new Error(`Conflicting immutable asset: ${name}; publish a new version`);
  }
  // GitHub may leave an empty starter after an upload 502. Delete only this
  // target's confirmed empty placeholders, by exact ID, after all preflight.
  for (const asset of starters) api.remove(asset);
  for (const name of missing) api.upload(tag, paths.get(name));

  // Re-read canonical paginated asset metadata, including other platforms and
  // previous failed attempts. Do not trust cached release download URLs/JSON.
  release = api.release(tag);
  validateRelease(release, tag);
  const manifest = manifestFromAssets({ api, release, tag, repo });
  if (!manifest.platforms[platform]) throw new Error('Current platform is not ready to advertise');
  const temp = mkdtempSync(join(tmpdir(), 'airtype-updater-manifest-'));
  try {
    const path = join(temp, 'latest.json');
    writeFileSync(path, `${JSON.stringify(manifest, null, 2)}\n`, { mode: 0o600 });
    // GitHub asset replacement is non-atomic; a failed replacement is recovered
    // by rerunning this publisher, rebuilding from the immutable signed sets.
    api.upload(tag, path, true);
  } finally {
    rmSync(temp, { recursive: true, force: true });
  }
  if (release.draft) {
    api.publish(tag);
    // Immutability can be applied only when a draft becomes public. Never
    // promote a partial release we can no longer extend. Do not change policy.
    release = api.release(tag);
    validateRelease(release, tag);
    if (release.draft) throw new Error('Release remained a draft after publication');
  }
  const currentLatest = api.latest();
  if (!currentLatest || newerOrEqual(tag, currentLatest.tag_name)) api.promote(tag);
  return manifest;
}

export function githubApi(repo, gh = (args) => execFileSync('gh', args, { encoding: 'utf8', timeout: 120_000,
  maxBuffer: 4 * 1024 * 1024, stdio: ['ignore', 'pipe', 'pipe'] })) {
  const optional = (endpoint) => {
    try { return JSON.parse(gh(['api', endpoint])); }
    catch (error) {
      if (error.status === 1 && /\(HTTP 404\)/.test(error.stderr?.toString() ?? '')) return null;
      throw error;
    }
  };
  const prefix = `repos/${repo}/releases`;
  const lines = (args) => gh(args).split('\n').filter(Boolean).map((line) => JSON.parse(line));
  return {
    latest: () => optional(`${prefix}/latest`),
    release: (tag) => {
      let found = optional(`${prefix}/tags/${tag}`);
      if (!found) {
        // The tag endpoint is documented for published releases. Authenticated
        // list-releases also includes drafts for this contents:write token.
        const matches = lines(['api', '--paginate', `${prefix}?per_page=100`,
          '--jq', '.[] | {id, tag_name} | @json']).filter((item) => item.tag_name === tag);
        if (matches.length > 1) throw new Error(`Multiple releases for tag ${tag}`);
        found = matches[0];
      }
      if (!found) return null;
      if (!Number.isSafeInteger(found.id) || found.id <= 0) throw new Error('Invalid release ID');
      const release = JSON.parse(gh(['api', `${prefix}/${found.id}`]));
      release.assets = lines(['api', '--paginate', `${prefix}/${found.id}/assets?per_page=100`, '--jq', '.[] | @json']);
      return release;
    },
    create: (tag) => gh(['release', 'create', tag, '--repo', repo, '--verify-tag', '--draft',
      '--title', `AirType ${tag}`, '--generate-notes', '--latest=false']),
    signature: (asset) => gh(['api', `${prefix}/assets/${asset.id}`, '-H', 'Accept: application/octet-stream']),
    remove: (asset) => gh(['api', '--method', 'DELETE', `${prefix}/assets/${asset.id}`]),
    upload: (tag, path, replace = false) => gh(['release', 'upload', tag, path, '--repo', repo, ...(replace ? ['--clobber'] : [])]),
    publish: (tag) => gh(['release', 'edit', tag, '--repo', repo, '--draft=false', '--latest=false']),
    promote: (tag) => gh(['release', 'edit', tag, '--repo', repo, '--latest']),
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const { values } = parseArgs({ options: { repo: { type: 'string' }, tag: { type: 'string' },
      platform: { type: 'string' }, artifacts: { type: 'string' } } });
    for (const key of ['repo', 'tag', 'platform', 'artifacts']) if (!values[key]) throw new Error(`Missing --${key}`);
    checkReleaseVersion(process.cwd(), values.tag);
    const manifest = publishPlatform({ repo: values.repo, tag: values.tag, platform: values.platform,
      directory: resolve(values.artifacts), api: githubApi(values.repo) });
    const message = `Published ${values.tag}: ${Object.keys(manifest.platforms).join(', ')} (${Object.keys(manifest.platforms).length}/3 platforms)`;
    console.log(message);
    if (process.env.GITHUB_STEP_SUMMARY) writeFileSync(process.env.GITHUB_STEP_SUMMARY, `${message}\n`, { flag: 'a' });
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
