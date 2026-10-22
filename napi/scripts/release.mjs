import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { createRequire } from 'node:module';
import { fileURLToPath, pathToFileURL } from 'node:url';
import semver from 'semver';
import { targets, validateTargets } from './targets.mjs';
import { checkPackage } from '../../.github/scripts/wait-for-npm-packages.mjs';

const root = fileURLToPath(new URL('..', import.meta.url));
const require = createRequire(import.meta.url);
const readJson = file => JSON.parse(fs.readFileSync(file, 'utf8'));
const writeJson = (file, value) => fs.writeFileSync(file, JSON.stringify(value, null, 2) + '\n');
const manifest = () => readJson(path.join(root, 'package.json'));
const cli = path.join(path.dirname(require.resolve('@napi-rs/cli/package.json')), 'cli.mjs');
const napi = args => execFileSync(process.execPath, [cli, ...args], { cwd: root, stdio: 'inherit' });

export function pnpm(args, cwd = root) {
  // pnpm is a .cmd shim on Windows; use its JS entrypoint when invoked by pnpm run.
  if (process.env.npm_execpath?.endsWith('.js') || process.env.npm_execpath?.endsWith('.cjs')) {
    return execFileSync(process.execPath, [process.env.npm_execpath, ...args], { cwd, stdio: 'inherit' });
  }
  return execFileSync(process.platform === 'win32' ? 'pnpm.cmd' : 'pnpm', args, { cwd, stdio: 'inherit', shell: process.platform === 'win32' });
}

export async function registryVersion(name, version = 'latest', fetchPackage = fetch) {
  const response = await fetchPackage(`https://registry.npmjs.org/${encodeURIComponent(name)}/${encodeURIComponent(version)}`, { signal: AbortSignal.timeout(30_000) });
  if (response.status === 404) return null;
  if (!response.ok) throw new Error(`Registry request failed: ${response.status} for ${name}@${version}`);
  const data = await response.json();
  if (data.name !== name || !semver.valid(data.version)) throw new Error(`Invalid registry metadata for ${name}`);
  return data.version;
}

export async function needsRelease(pkg, fetchPackage = fetch) {
  if (!semver.valid(pkg.version) || semver.prerelease(pkg.version)) throw new Error('N-API releases require a stable semver version');
  const latest = await registryVersion(pkg.name, 'latest', fetchPackage);
  if (latest && semver.lt(pkg.version, latest)) throw new Error(`Refusing to replace ${latest} with older ${pkg.version}`);
  return latest !== pkg.version;
}

function filesBelow(dir) {
  return fs.readdirSync(dir, { withFileTypes: true }).flatMap(entry => {
    const file = path.join(dir, entry.name);
    return entry.isDirectory() ? filesBelow(file) : [file];
  });
}

export function validateArtifacts(pkg, files, selected = targets) {
  validateTargets(pkg.napi.targets);
  for (const { suffix } of selected) {
    const name = `${pkg.napi.binaryName}.${suffix}.node`;
    const matches = files.filter(file => path.basename(file) === name);
    if (matches.length !== 1 || fs.statSync(matches[0]).size === 0) throw new Error(`Expected exactly one nonempty ${name}`);
  }
}

// Assembly never invokes pre-publish: that command has registry side effects. Publish only the
// already validated tarballs, with the exact same optionalDependencies in local consumer checks.
export function assemble(artifacts = path.join(root, 'artifacts'), selected = targets) {
  const pkg = manifest();
  const files = filesBelow(artifacts);
  validateArtifacts(pkg, files, selected);
  const loaders = files.filter(file => path.basename(file) === 'binding.cjs');
  if (!loaders.length) throw new Error('Missing generated binding.cjs artifact');
  const loader = fs.readFileSync(loaders[0]);
  if (loaders.some(file => !fs.readFileSync(file).equals(loader))) throw new Error('Generated loaders differ between builds');
  const dist = path.join(root, 'dist');
  fs.rmSync(dist, { recursive: true, force: true });
  const main = path.join(dist, 'package');
  fs.mkdirSync(path.join(main, 'src-js'), { recursive: true });
  for (const name of ['index.cjs', 'index.mjs', 'index.d.cts', 'index.d.mts']) {
    fs.copyFileSync(path.join(root, 'src-js', name), path.join(main, 'src-js', name));
  }
  fs.writeFileSync(path.join(main, 'src-js/binding.cjs'), loader);
  fs.copyFileSync(path.join(root, '../LICENSE'), path.join(main, 'LICENSE'));
  fs.copyFileSync(path.join(root, '../NOTICE'), path.join(main, 'NOTICE'));
  fs.copyFileSync(path.join(root, 'README.md'), path.join(main, 'README.md'));
  const staged = { ...pkg, napi: { ...pkg.napi, targets: selected.map(t => t.target) } };
  delete staged.scripts;
  delete staged.devDependencies;
  const packageJson = path.join(main, 'package.json');
  writeJson(packageJson, staged);
  const npmDir = path.join(main, 'npm');
  napi(['create-npm-dirs', '--package-json-path', packageJson, '--npm-dir', npmDir]);
  napi(['artifacts', '--package-json-path', packageJson, '--npm-dir', npmDir, '--output-dir', artifacts]);
  const packages = selected.map(({ suffix }) => {
    const dir = path.join(npmDir, suffix);
    const platform = readJson(path.join(dir, 'package.json'));
    const binary = `${pkg.napi.binaryName}.${suffix}.node`;
    if (platform.name !== `${pkg.name}-${suffix}` || platform.version !== pkg.version || !fs.statSync(path.join(dir, binary)).size) {
      throw new Error(`Invalid platform package ${suffix}`);
    }
    fs.copyFileSync(path.join(root, '../LICENSE'), path.join(dir, 'LICENSE'));
    fs.copyFileSync(path.join(root, '../NOTICE'), path.join(dir, 'NOTICE'));
    platform.files = [...new Set([...(platform.files ?? [binary]), 'LICENSE', 'NOTICE'])];
    writeJson(path.join(dir, 'package.json'), platform);
    return { name: platform.name, dir };
  });
  staged.optionalDependencies = Object.fromEntries(packages.map(p => [p.name, pkg.version]));
  delete staged.napi;
  writeJson(packageJson, staged);
  packages.push({ name: pkg.name, dir: main });
  const tarballs = path.join(dist, 'tarballs');
  fs.mkdirSync(tarballs);
  for (const p of packages) {
    const before = new Set(fs.readdirSync(tarballs));
    pnpm(['pack', '--pack-destination', tarballs], p.dir);
    const added = fs.readdirSync(tarballs).filter(file => !before.has(file) && file.endsWith('.tgz'));
    if (added.length !== 1) throw new Error(`Expected one tarball for ${p.name}`);
    p.tarball = path.join('tarballs', added[0]);
    p.dir = path.relative(dist, p.dir);
  }
  const result = { version: pkg.version, packages };
  writeJson(path.join(dist, 'packages.json'), result);
  return result;
}

export async function publishPackages(release, { view = registryVersion, publish, wait } = {}) {
  for (const pkg of release.packages) {
    const found = await view(pkg.name, release.version);
    if (found && found !== release.version) throw new Error(`Unexpected version for ${pkg.name}`);
    if (!found) await publish(pkg);
    // Root is last; all its platform packages are visible before it is published.
    await wait(pkg);
  }
}

async function waitFor(pkg, version) {
  const deadline = Date.now() + 30 * 60 * 1000;
  while (true) {
    try { await checkPackage([pkg.name, version], true); return; }
    catch (error) {
      if (Date.now() >= deadline) throw error;
      await new Promise(resolve => setTimeout(resolve, 10_000));
    }
  }
}

async function main() {
  const pkg = manifest();
  validateTargets(pkg.napi.targets);
  switch (process.argv[2]) {
    case 'version': {
      const release = await needsRelease(pkg);
      const output = `release=${release}\nversion=${pkg.version}\nmatrix=${JSON.stringify({ include: targets })}\n`;
      if (process.env.GITHUB_OUTPUT) fs.appendFileSync(process.env.GITHUB_OUTPUT, output);
      else process.stdout.write(output);
      break;
    }
    case 'assemble': {
      const selected = process.argv[4] ? targets.filter(t => t.target === process.argv[4]) : targets;
      if (!selected.length) throw new Error('Unknown target');
      assemble(process.argv[3] ? path.resolve(process.argv[3]) : undefined, selected);
      break;
    }
    case 'publish': {
      if (process.env.GITHUB_REPOSITORY !== 'oxc-project/tsrs' || process.env.GITHUB_REF !== 'refs/heads/main') {
        throw new Error('Publishing requires oxc-project/tsrs main');
      }
      const release = readJson(path.join(root, 'dist/packages.json'));
      const expected = [...targets.map(t => `${pkg.name}-${t.suffix}`), pkg.name];
      if (release.version !== pkg.version || JSON.stringify(release.packages.map(p => p.name)) !== JSON.stringify(expected)) {
        throw new Error('Incomplete release manifest');
      }
      await needsRelease(pkg); // reject version rollback, while allowing interrupted releases to resume
      await publishPackages(release, {
        publish: p => pnpm(['publish', path.join(root, 'dist', p.tarball), '--access', 'public', '--tag', 'latest', '--no-git-checks', ...(process.env.PRIVATE_REPO === 'true' ? [] : ['--provenance'])]),
        wait: p => waitFor(p, release.version),
      });
      break;
    }
    default: throw new Error('Expected version, assemble or publish');
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(path.resolve(process.argv[1])).href) {
  main().catch(error => { console.error(error); process.exitCode = 1; });
}
