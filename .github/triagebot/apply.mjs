import { execFileSync } from 'node:child_process';
import { copyFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const overlay = dirname(fileURLToPath(import.meta.url));
const action = resolve(process.argv[2] ?? 'target/triagebot-action');
// --check makes a pin/source mismatch fail before making any changes.
execFileSync('git', ['apply', '--check', resolve(overlay, 'upstream.patch')], { cwd: action, stdio: 'inherit' });
execFileSync('git', ['apply', resolve(overlay, 'upstream.patch')], { cwd: action, stdio: 'inherit' });
mkdirSync(resolve(action, 'src/tsrs'), { recursive: true });
for (const [source, destination] of [
    ['comment.ts', 'src/handlers/comment.ts'],
    ['pipeline.ts', 'src/tsrs/pipeline.ts'],
    ['stages.ts', 'src/tsrs/stages.ts'],
]) copyFileSync(resolve(overlay, source), resolve(action, destination));
