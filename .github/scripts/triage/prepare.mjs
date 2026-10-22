import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export function referenceCommit(root) {
    const cargo = readFileSync(resolve(root, 'Cargo.toml'), 'utf8');
    const metadata = cargo.match(/^\[workspace\.metadata\.typescript\]\s*\n([\s\S]*?)(?=^\[|$(?![\s\S]))/m)?.[1];
    const commit = metadata?.match(/^commit\s*=\s*"([a-f0-9]{40})"/m)?.[1];
    if (!commit) throw new Error('Missing exact TypeScript commit in Cargo.toml');
    return commit;
}

export function run(command, args, options = {}) {
    const output = execFileSync(command, args, {
        encoding: 'utf8', stdio: ['ignore', 'pipe', 'inherit'], ...options,
    });
    return output?.trim() ?? '';
}

export function prepareReference(root, execute = run) {
    const commit = referenceCommit(root);
    const referenceRoot = resolve(root, 'ts-ref');
    const output = resolve(root, 'target/triage');
    const goBinary = resolve(output, process.platform === 'win32' ? 'tsgo.exe' : 'tsgo');
    const manifestPath = resolve(output, 'reference.json');
    mkdirSync(output, { recursive: true });
    if (!existsSync(resolve(referenceRoot, '.git'))) {
        mkdirSync(referenceRoot, { recursive: true });
        execute('git', ['init', referenceRoot]);
        execute('git', ['-C', referenceRoot, 'remote', 'add', 'origin', 'https://github.com/microsoft/TypeScript.git']);
    }
    let head;
    try { head = execute('git', ['-C', referenceRoot, 'rev-parse', 'HEAD']); } catch { /* New checkout. */ }
    if (head !== commit) {
        execute('git', ['-C', referenceRoot, 'fetch', '--depth=1', 'origin', commit]);
        execute('git', ['-C', referenceRoot, 'sparse-checkout', 'set',
            'tsc', 'packages/typescript/src', 'packages/typescript/lib',
            'packages/typescript/vendor', 'tools/scripts/tsc']);
        execute('git', ['-C', referenceRoot, 'checkout', '--detach', commit]);
    }
    if (execute('git', ['-C', referenceRoot, 'rev-parse', 'HEAD']) !== commit) {
        throw new Error('Reference checkout does not match the pinned commit');
    }
    if (execute('git', ['-C', referenceRoot, 'status', '--porcelain', '--untracked-files=no'])) {
        throw new Error('Reference checkout has tracked modifications');
    }
    let previous;
    try { previous = JSON.parse(readFileSync(manifestPath, 'utf8')); } catch { /* Cold cache. */ }
    const goMod = readFileSync(resolve(referenceRoot, 'tsc/go.mod'), 'utf8');
    const requiredGo = goMod.match(/^go\s+(\S+)/m)?.[1];
    if (!requiredGo) throw new Error('Reference go.mod has no Go version');
    const identity = { commit, requiredGo, platform: process.platform, arch: process.arch };
    if (existsSync(goBinary) && previous?.goVersion &&
        Object.entries(identity).every(([key, value]) => previous[key] === value)) {
        execute(goBinary, ['--version']);
        return { ...previous, referenceRoot, goBinary };
    }
    const options = { cwd: resolve(referenceRoot, 'tsc'), env: { ...process.env, GOTOOLCHAIN: 'auto', GOWORK: 'off' } };
    execute('go', ['build', '-o', goBinary, './cmd/tsc'], options);
    const goVersion = execute('go', ['env', 'GOVERSION'], options);
    const manifest = { ...identity, goVersion, referenceRoot, goBinary };
    writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
    return manifest;
}

export function prepare(root, { referenceOnly = false, execute = run } = {}) {
    const reference = prepareReference(root, execute);
    if (referenceOnly) return reference;
    execute('cargo', ['build', '--profile', 'dev', '--locked', '-p', 'tsrs_cli', '-p', 'tsrs_testrunner'], {
        cwd: root, stdio: 'inherit',
    });
    const manifest = {
        ...reference, tsrsCommit: execute('git', ['rev-parse', 'HEAD'], { cwd: root }),
        rustBinary: resolve(root, 'target/debug/tsrs'), testBinary: resolve(root, 'target/debug/tsrs-test'),
    };
    writeFileSync(resolve(root, 'target/triage/compilers.json'), JSON.stringify(manifest, null, 2) + '\n');
    return manifest;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
    console.log(JSON.stringify(prepare(process.cwd(), { referenceOnly: process.argv.includes('--reference-only') }), null, 2));
}
