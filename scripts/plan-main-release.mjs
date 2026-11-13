import { execFileSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { checkReleaseVersion } from './check-release-version.mjs';

export function planMainRelease({ version, sha, ref, tagSha }) {
  if (ref !== 'refs/heads/main') {
    throw new Error('Automatic releases must target main.');
  }
  if (typeof version !== 'string' || !/^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)$/.test(version)) {
    throw new Error('Automatic releases require a stable semantic version (major.minor.patch).');
  }
  const commit = /^[a-f0-9]{40}$/;
  if (typeof sha !== 'string' || !commit.test(sha)
      || tagSha !== null && (typeof tagSha !== 'string' || !commit.test(tagSha))) {
    throw new Error('Release targets must be full commit SHAs.');
  }
  return {
    tag: `v${version}`,
    sha,
    create_tag: tagSha === null,
    dispatch: tagSha === null || tagSha === sha,
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const root = process.cwd();
    const version = checkReleaseVersion(root);
    const sha = process.env.GITHUB_SHA;
    const ref = process.env.GITHUB_REF;
    const initial = planMainRelease({ version, sha, ref, tagSha: null });
    const git = (args) => execFileSync('git', args, { cwd: root, encoding: 'utf8' }).trim();
    if (git(['rev-parse', '--verify', 'HEAD']) !== sha) {
      throw new Error('Checked-out commit must match the triggering commit SHA.');
    }
    const present = git(['tag', '--list', initial.tag]);
    const tagSha = present ? git(['rev-parse', '--verify', `refs/tags/${initial.tag}^{commit}`]) : null;
    const plan = planMainRelease({ version, sha, ref, tagSha });
    if (process.env.GITHUB_OUTPUT) {
      appendFileSync(process.env.GITHUB_OUTPUT,
        Object.entries(plan).map(([key, value]) => `${key}=${value}\n`).join(''));
    }
    console.log(JSON.stringify(plan));
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
