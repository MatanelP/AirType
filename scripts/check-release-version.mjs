import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export function checkReleaseVersion(root, tag) {
  const read = (file) => readFileSync(resolve(root, file), 'utf8');
  const pkg = JSON.parse(read('package.json'));
  const lock = JSON.parse(read('package-lock.json'));
  const cargoPackage = read('src-tauri/Cargo.toml')
    .split(/^\[package\]\s*$/m)[1]?.split(/^\[/m)[0];
  const cargoVersion = cargoPackage?.match(/^version\s*=\s*"([^"]+)"/m)?.[1];
  const cargoLockPackage = read('src-tauri/Cargo.lock').split('[[package]]')
    .find((block) => /^name = "airtype"$/m.test(block));
  const versions = {
    'package.json': pkg.version,
    'package-lock.json': lock.version,
    'package-lock.json root package': lock.packages?.['']?.version,
    'src-tauri/Cargo.toml': cargoVersion,
    'src-tauri/Cargo.lock': cargoLockPackage?.match(/^version = "([^"]+)"/m)?.[1],
    'src-tauri/tauri.conf.json': JSON.parse(read('src-tauri/tauri.conf.json')).version,
  };
  if (!pkg.version || Object.values(versions).some((version) => version !== pkg.version)) {
    throw new Error(`App versions must match: ${JSON.stringify(versions)}`);
  }
  if (tag !== undefined && tag !== `v${pkg.version}`) {
    throw new Error(`Release tag ${tag} must match app version v${pkg.version}`);
  }
  return pkg.version;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const tag = process.env.GITHUB_REF_TYPE === 'tag' ? process.env.GITHUB_REF_NAME : undefined;
    console.log(`Validated app version ${checkReleaseVersion(process.cwd(), tag)}`);
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
