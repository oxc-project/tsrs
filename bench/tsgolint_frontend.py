#!/usr/bin/env python3
"""Compare Oxlint helpers through the complete frontend on macOS."""

import argparse
import collections
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

from tsgolint import counters, digest


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--frontend', type=Path, required=True)
    parser.add_argument('--config', type=Path, required=True)
    parser.add_argument('--cwd', type=Path, required=True)
    parser.add_argument('--helper', action='append', required=True, metavar='NAME=PATH')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--runs', type=int, default=5)
    parser.add_argument('--warmups', type=int, default=1)
    args = parser.parse_args()
    if args.runs < 1 or args.warmups < 0:
        parser.error('runs must be positive and warmups nonnegative')
    helpers = {}
    for helper in args.helper:
        name, separator, path = helper.partition('=')
        if not separator or not re.fullmatch(r'[a-zA-Z0-9_-]+', name) or name in helpers:
            parser.error('--helper requires a unique NAME=PATH')
        helpers[name] = Path(path).resolve(strict=True)
    frontend = args.frontend.resolve(strict=True)
    config = args.config.resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    metadata = {
        'frontend': {'path': str(frontend), 'sha256': digest(frontend)},
        'config': {'path': str(config), 'sha256': digest(config)},
        'cwd': str(args.cwd.resolve()),
        'helpers': {name: {'path': str(path), 'sha256': digest(path)} for name, path in helpers.items()},
        'runs': args.runs, 'warmups': args.warmups,
        'mode': 'default; thread overrides unset',
        'rss_note': 'Frontend time -l counters are not helper-only or simultaneous process-tree peaks.',
    }
    (output / 'metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
    for variant in ('lint', 'lint-typecheck'):
        command = [str(frontend), '--type-aware', '--no-ignore', '-c', str(config), '--format', 'json', 'src']
        if variant == 'lint-typecheck':
            command.append('--type-check')
        for repetition in range(-args.warmups, args.runs):
            order = list(helpers)
            if repetition >= 0 and repetition % 2:
                order.reverse()
            for name in order:
                label = f'{variant}-{repetition}-{name}'
                env = os.environ.copy()
                for key in ('GOMAXPROCS', 'RAYON_NUM_THREADS', 'TSRS_CHECKERS', 'TSRS_HISTORY', 'TS_TEST_PROGRAM_SINGLE_THREADED', 'OXLINT_TSGOLINT_DANGEROUSLY_SUPPRESS_PROGRAM_DIAGNOSTICS', 'OXLINT_TSGOLINT_CHECKER_SCHEDULING'):
                    env.pop(key, None)
                env['OXLINT_TSGOLINT_PATH'] = str(helpers[name])
                started = time.perf_counter()
                with (output / f'{label}.json').open('wb') as stdout, (output / f'{label}.err').open('wb') as stderr:
                    process = subprocess.Popen(['/usr/bin/time', '-l', '-o', str(output / f'{label}.time'), *command], cwd=args.cwd, env=env, stdin=subprocess.DEVNULL, stdout=stdout, stderr=stderr, start_new_session=True)
                    try:
                        status = process.wait(timeout=900)
                    except BaseException:
                        os.killpg(process.pid, signal.SIGKILL)
                        process.wait()
                        raise
                wall = time.perf_counter() - started
                if status not in (0, 1):
                    raise RuntimeError(f'{label} exited {status}; inspect {output / (label + ".err")}')
                result = json.loads((output / f'{label}.json').read_bytes())
                records = collections.Counter()
                counts = collections.Counter()
                for diagnostic in result['diagnostics']:
                    canonical = json.dumps(diagnostic, sort_keys=True, separators=(',', ':')).encode()
                    records[hashlib.sha256(canonical).hexdigest()] += 1
                    counts[diagnostic.get('code') or '<frontend>'] += 1
                multiset = json.dumps(sorted(records.items()), separators=(',', ':')).encode()
                sample = {
                    'helper': name, 'variant': variant, 'rep': repetition, 'warmup': repetition < 0,
                    'wall_s': wall, 'exit_code': status, **counters(output / f'{label}.time'),
                    'diagnostics': len(result['diagnostics']), 'diagnostics_by_code': dict(sorted(counts.items())),
                    'diagnostic_multiset_sha256': hashlib.sha256(multiset).hexdigest(),
                    'reported_files': result.get('number_of_files'),
                }
                with (output / 'samples.jsonl').open('a') as stream:
                    stream.write(json.dumps(sample) + '\n')
                print(f'{label}: {wall:.3f}s, {sample["diagnostics"]} diagnostics', flush=True)


if __name__ == '__main__':
    main()
