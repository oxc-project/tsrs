// node repro.mjs tsrs|typescript
// Resolves the same module name from the same directory 60,000 times through
// one ModuleResolver, then disposes it, printing the API server's RSS.
import fs from 'node:fs';
import path from 'node:path';
import {execFileSync} from 'node:child_process';

const impl = process.argv[2] ?? 'tsrs';
const {API, ModuleKind, ModuleResolutionKind} = await import(`${impl}/unstable/sync`);

const root = path.resolve('fixture');
fs.mkdirSync(path.join(root, 'src'), {recursive: true});
fs.writeFileSync(path.join(root, 'src/index.ts'), 'export {};\n');

const serverRssMb = () => {
  const pid = execFileSync('ps', ['-o', 'pid=', '--ppid', String(process.pid)], {encoding: 'utf8'}).trim().split(/\s+/)[0];
  return Math.round(Number(fs.readFileSync(`/proc/${pid}/status`, 'utf8').match(/VmRSS:\s+(\d+)/)[1]) / 1024);
};

const api = new API({cwd: root});
const resolver = api.createModuleResolver({
  module: ModuleKind.ESNext,
  moduleResolution: ModuleResolutionKind.Bundler,
});
console.log(`${impl}: start ${serverRssMb()} MB`);
for (let i = 1; i <= 60_000; i++) {
  resolver.resolveModuleName('./index.ts', path.join(root, 'src'));
  if (i % 20_000 === 0) console.log(`${impl}: ${i} resolutions ${serverRssMb()} MB`);
}
resolver.dispose();
await new Promise((resolve) => setTimeout(resolve, 1000));
console.log(`${impl}: after dispose ${serverRssMb()} MB`);
api.close();
