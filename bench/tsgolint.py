#!/usr/bin/env python3
"""Benchmark headless helpers on one payload, with and without full type checking (macOS)."""

import argparse
import collections
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import struct
import subprocess
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def summarize(path):
    counts = collections.Counter()
    records = collections.Counter()
    with path.open('rb') as stream:
        while header := stream.read(5):
            if len(header) != 5:
                raise ValueError('short frame header')
            length, kind = struct.unpack('<IB', header)
            body = stream.read(length)
            if len(body) != length:
                raise ValueError('short frame body')
            value = json.loads(body)
            if kind == 0:
                raise ValueError(f'helper error: {value}')
            if kind == 1:
                counts[value.get('rule') or '<compiler>'] += 1
                canonical = json.dumps(value, sort_keys=True, separators=(',', ':')).encode()
                records[hashlib.sha256(canonical).hexdigest()] += 1
    multiset = json.dumps(sorted(records.items()), separators=(',', ':')).encode()
    return {
        'diagnostics': sum(counts.values()),
        'diagnostics_by_rule': dict(sorted(counts.items())),
        'diagnostic_multiset_sha256': hashlib.sha256(multiset).hexdigest(),
    }


def counters(path):
    text = path.read_text()
    result = {}
    for key, pattern in [
        ('rss_bytes', r'(\d+)\s+maximum resident set size'),
        ('instructions', r'(\d+)\s+instructions retired'),
        ('cpu_cycles', r'(\d+)\s+cycles elapsed'),
    ]:
        match = re.search(pattern, text)
        if match:
            result[key] = int(match[1])
    match = re.search(r'([\d.]+)\s+real\s+([\d.]+)\s+user\s+([\d.]+)\s+sys', text)
    if match:
        result.update(time_real=float(match[1]), user=float(match[2]), sys=float(match[3]))
    if 'instructions' not in result or 'rss_bytes' not in result:
        raise ValueError(f'missing macOS time counters in {path}')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--payload', type=Path, required=True)
    parser.add_argument('--cwd', type=Path, required=True)
    parser.add_argument('--helper', action='append', required=True, metavar='NAME=PATH')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--runs', type=int, default=3)
    parser.add_argument('--warmups', type=int, default=1)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument('--modes', nargs='+', choices=['default', 'single'], default=['default', 'single'])
    modes.add_argument('--threads', nargs='+', type=int, help='Interleave explicit worker limits instead of --modes')
    parser.add_argument('--variants', nargs='+', choices=['lint', 'lint-typecheck', 'typecheck', 'setup'], default=['lint', 'lint-typecheck'])
    args = parser.parse_args()
    if args.runs < 1 or args.warmups < 0:
        parser.error('runs must be positive and warmups nonnegative')
    if args.threads and (any(n < 1 or n > 256 for n in args.threads) or len(set(args.threads)) != len(args.threads)):
        parser.error('threads must be unique counts between 1 and 256')
    configurations = [(f'threads-{n}', n) for n in args.threads] if args.threads else [(mode, 1 if mode == 'single' else None) for mode in args.modes]
    helpers = {}
    for helper in args.helper:
        name, separator, path = helper.partition('=')
        if not separator or not re.fullmatch(r'[a-zA-Z0-9_-]+', name) or name in helpers:
            parser.error('--helper requires a unique NAME=PATH')
        helpers[name] = Path(path).resolve(strict=True)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = json.loads(args.payload.read_text())
    if source.get('version') != 2:
        parser.error('a version 2 payload is required')
    inputs = {}
    for variant in args.variants:
        payload = json.loads(json.dumps(source))
        payload['report_semantic'] = payload['report_syntactic'] = variant in ('lint-typecheck', 'typecheck')
        if variant in ('typecheck', 'setup'):
            for config in payload['configs']:
                config['rules'] = []
        path = output / f'{variant}.json'
        path.write_text(json.dumps(payload))
        inputs[variant] = path
    metadata = {
        'source_payload_sha256': digest(args.payload),
        'cwd': str(args.cwd.resolve()),
        'helpers': {name: {'path': str(path), 'sha256': digest(path)} for name, path in helpers.items()},
        'variants': {name: {'sha256': digest(path)} for name, path in inputs.items()},
        'runs': args.runs, 'warmups': args.warmups, 'modes': [mode for mode, _ in configurations], 'threads': args.threads,
        'single_mode': 'GOMAXPROCS=1 RAYON_NUM_THREADS=1; Go may retain multiple checker states',
    }
    (output / 'metadata.json').write_text(json.dumps(metadata, indent=2) + '\n')
    for variant, payload in inputs.items():
        for repetition in range(-args.warmups, args.runs):
            offset = max(repetition, 0) % len(configurations)
            for mode, threads in configurations[offset:] + configurations[:offset]:
                order = list(helpers)
                if repetition >= 0 and repetition % 2:
                    order.reverse()
                for name in order:
                    label = f'{variant}-{mode}-{repetition}-{name}'
                    env = os.environ.copy()
                    for key in ('GOMAXPROCS', 'RAYON_NUM_THREADS', 'TSRS_CHECKERS', 'TSRS_HISTORY', 'TS_TEST_PROGRAM_SINGLE_THREADED', 'OXLINT_TSGOLINT_DANGEROUSLY_SUPPRESS_PROGRAM_DIAGNOSTICS', 'OXLINT_TSGOLINT_CHECKER_SCHEDULING'):
                        env.pop(key, None)
                    if threads is not None:
                        env['RAYON_NUM_THREADS'] = str(threads)
                        env['GOMAXPROCS'] = str(threads)
                    started = time.perf_counter()
                    with payload.open('rb') as stdin, (output / f'{label}.out').open('wb') as stdout, (output / f'{label}.err').open('wb') as stderr:
                        process = subprocess.Popen(['/usr/bin/time', '-l', '-o', str(output / f'{label}.time'), str(helpers[name]), 'headless'], cwd=args.cwd, env=env, stdin=stdin, stdout=stdout, stderr=stderr, start_new_session=True)
                        try:
                            status = process.wait(timeout=900)
                        except BaseException:
                            os.killpg(process.pid, signal.SIGKILL)
                            process.wait()
                            raise
                    wall = time.perf_counter() - started
                    if status:
                        raise RuntimeError(f'{label} exited {status}; inspect {output / (label + ".err")}')
                    sample = {'helper': name, 'variant': variant, 'mode': mode, 'rep': repetition, 'threads': threads, 'warmup': repetition < 0, 'wall_s': wall, 'exit_code': status, **counters(output / f'{label}.time'), **summarize(output / f'{label}.out')}
                    with (output / 'samples.jsonl').open('a') as stream:
                        stream.write(json.dumps(sample) + '\n')
                    print(f'{label}: {wall:.3f}s, {sample["rss_bytes"] / 2**20:.1f} MiB, {sample["diagnostics"]} diagnostics', flush=True)


if __name__ == '__main__':
    main()
