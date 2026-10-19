"""Offline coverage for Hyperfine execution, provenance and reproducible blog exports."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import laptop
from results import load_bundle

ARCHIVE = Path(__file__).parent / 'laptop-results/20261019T104740.804330Z'


class BundleTests(unittest.TestCase):
    def test_all_archived_samples_match_exports(self):
        meta, rows, checks = load_bundle(ARCHIVE)
        self.assertEqual(240, len(rows))
        self.assertEqual(8, len(checks))
        self.assertEqual('0.17.0-ts7.1.0-dev.20260929', meta['compilers']['tsrs']['version'])

    def test_rejects_corrupt_or_incomplete_inputs(self):
        cases = {
            'incomplete': ('metadata.json', lambda d: d.pop('completed_utc')),
            'duplicate cell': ('metadata.json', lambda d: d['cells'].append(d['cells'][0])),
            'raw instructions': ('raw.json', lambda d: d[0].update(instructions=d[0]['instructions'] + 1)),
            'raw exit code': ('raw.json', lambda d: d[0].update(exit_code=42)),
            'wrong units': ('aligned/vscode/hyperfine.json', lambda d:
                           d['results'][0]['measurements'][0]['memory_peak_resident'].update(unit='KiB')),
            'wrong command': ('aligned/vscode/hyperfine.json', lambda d:
                             d['results'][0].update(command='another-compiler')),
            'missing run': ('aligned/vscode/hyperfine.json', lambda d: d['results'][0]['measurements'].pop()),
            'changed preflight': ('aligned/vscode/preflight.json', lambda d:
                                 d['compilers']['typescript'].update(exit_code=42)),
        }
        for label, (filename, mutate) in cases.items():
            with self.subTest(label=label), tempfile.TemporaryDirectory() as directory:
                source = Path(directory) / 'copied'
                shutil.copytree(ARCHIVE, source, ignore=shutil.ignore_patterns('charts'))
                path = source / filename
                data = json.loads(path.read_text())
                mutate(data)
                path.write_text(json.dumps(data))
                with self.assertRaises(ValueError):
                    load_bundle(source)
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / 'copied'
            shutil.copytree(ARCHIVE, source, ignore=shutil.ignore_patterns('charts'))
            path = source / 'raw.csv'
            path.write_text(path.read_text().replace('aligned', 'maximum', 1))
            with self.assertRaisesRegex(ValueError, 'raw.csv'):
                load_bundle(source)


class BashRunnerTests(unittest.TestCase):
    def test_replay_installs_archived_lockfiles_without_resolving_tags(self):
        with tempfile.TemporaryDirectory() as directory:
            work = Path(directory)
            destination = work / 'saved'
            destination.mkdir()
            installed = []

            def npm(argv, **kwargs):
                self.assertEqual(['npm', 'ci'], argv[:2])
                install = Path(argv[argv.index('--prefix') + 1])
                lock = json.loads((install / 'package-lock.json').read_text())
                root = json.loads((install / 'package.json').read_text())
                self.assertEqual(root, lock['packages'][''])
                installed.append(lock)
                for name, version in root['dependencies'].items():
                    package = install / 'node_modules' / name
                    package.mkdir(parents=True, exist_ok=True)
                    laptop.write_json(package / 'package.json', {'version': version})
                    if name.startswith(('@typescript/', '@tsrs/')):
                        binary = package / ('tsrs' if name.startswith('@tsrs/') else 'lib/tsc')
                        binary.parent.mkdir(parents=True, exist_ok=True)
                        binary.write_bytes(b'\xcf\xfa\xed\xfe' + b'fixture')

            with patch.object(laptop, 'release', side_effect=AssertionError('resolved a moving tag')), \
                    patch.object(laptop.subprocess, 'run', side_effect=npm), \
                    patch.object(laptop, 'output', return_value='fixture version'):
                executables, packages = laptop.install_compilers(work, destination, ARCHIVE)
            self.assertEqual(3, len(installed))
            self.assertEqual({'tsrs', 'typescript', 'typescript_7_0_2'}, set(executables))
            for name in executables:
                self.assertEqual((ARCHIVE / f'{name}-package-lock.json').read_bytes(),
                                 (destination / f'{name}-package-lock.json').read_bytes())
                self.assertNotIn('@next', packages[name]['requested'])
                self.assertNotIn('@latest', packages[name]['requested'])

    def test_generated_bash_executes_and_records_the_printed_matrix(self):
        # Spaces and shell metacharacters must survive both Bash quoting and Hyperfine's command parser.
        with tempfile.TemporaryDirectory(prefix='bench $literal ') as directory:
            work = Path(directory).resolve()
            subprocess.run(['git', 'init', '-q', str(work)], check=True)
            subprocess.run(['git', '-c', 'commit.gpgsign=false', '-c', 'user.name=Fixture', '-c', 'user.email=fixture@example.invalid',
                            'commit', '-q', '--allow-empty', '-m', 'fixture'], cwd=work, check=True)
            commit = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=work, text=True).strip()
            compiler = work / 'compiler $literal'
            compiler.write_text('#!/usr/bin/env python3\nprint("Files: 1")\n')
            compiler.chmod(0o755)
            hf = work / 'hyperfine $literal'
            hf.write_text('''#!/usr/bin/env python3
import json, os, shlex, sys
from pathlib import Path
args = sys.argv[1:]
get = lambda flag: args[args.index(flag) + 1]
path = Path(get('--export-json'))
path.parent.mkdir(parents=True, exist_ok=True)
names = [args[i+1:i+3] for i, a in enumerate(args) if a == '--command-name'] or [('probe', '/usr/bin/true')]
sample = {'time_wall_clock': {'value': 0.125, 'unit': 'second'},
          'instructions': {'value': 12345}, 'memory_peak_resident': {'value': 4096, 'unit': 'byte'}, 'exit_code': 0}
results = [{'name': n, 'command': c, 'measurements': [sample] * int(get('--runs'))} for n, c in names]
path.write_text(json.dumps({'schema_version': 2, 'results': results}))
if '--output' in args:
    Path(get('--output')).write_text('Compiler output\\n')
    (path.parent / 'observed.json').write_text(json.dumps({'argv': args, 'cwd': os.getcwd(),
        'env': {k:v for k,v in os.environ.items() if k.startswith(('GO', 'RAYON_', 'TSRS_'))}}))
print('STUB_HYPERFINE_FINISHED', flush=True)
''')
            hf.chmod(0o755)
            projects = []
            for name, config in zip(laptop.PROJECTS, ('src', 'docs', '.', 'apps/server')):
                (work / 'solutions' / name / config).mkdir(parents=True)
                projects.append({'name': name, 'project': config, 'commit': commit, 'repo': 'fixture'})
            modes = {'aligned': {'label': 'Aligned', 'checkers': 4, 'memory_gib': None},
                     'maximum': {'label': 'Maximum resources', 'checkers': 3, 'memory_gib': 7}}
            executables = {c: compiler for c in ('typescript_7_0_2', 'typescript', 'tsrs')}
            cells = laptop.plan_cells({}, projects, modes, executables, work, hf, 2, 1, 3)
            metadata = {'machine': {'cores': 3}, 'projects': projects, 'modes': modes, 'runs': 2, 'warmup': 1,
                        'compilers': {c: {'binary': str(p), 'sha256': laptop.sha256(p)} for c, p in executables.items()},
                        'hyperfine': {'sha256': laptop.sha256(hf)}, 'cells': cells}
            plan = work / 'plan'
            plan.mkdir()
            laptop.write_json(plan / 'metadata.json', metadata)
            runner = plan / 'run.sh'
            runner.write_text(laptop.bash_runner(plan, work, metadata, hf))
            subprocess.run(['bash', '-n', str(runner)], check=True)
            dry = subprocess.check_output(['bash', str(runner), '--dry-run'], text=True)
            self.assertFalse((work / 'results').exists())
            self.assertEqual(9, dry.count('--export-json'))  # Counter probe plus eight cells.
            env = dict(os.environ, TSRS_UNWANTED='yes', GOGC='1', GOMEMLIMIT='1GiB', RAYON_NUM_THREADS='99')
            executed = subprocess.run(['bash', str(runner)], env=env, capture_output=True, text=True, check=True)
            # Every command is printed before its invocation, not after buffering the subprocess output.
            chunks = executed.stdout.split('STUB_HYPERFINE_FINISHED')
            self.assertEqual(10, len(chunks))
            for chunk in chunks[:-1]:
                self.assertIn('--export-json', chunk)
            source, = (work / 'results').iterdir()
            saved, rows, _ = load_bundle(source)
            self.assertEqual(48, len(rows))
            for cell in saved['cells']:
                observed = json.loads((source / cell['mode'] / cell['project'] / 'observed.json').read_text())
                self.assertEqual(cell['cwd'], observed['cwd'])
                self.assertEqual(cell['hyperfine_argv'][1:], observed['argv'])
                self.assertNotIn('TSRS_UNWANTED', observed['env'])
                self.assertNotIn('GOGC', observed['env'])
                for key, value in cell['environment'].items():
                    self.assertEqual(value, observed['env'].get(key))
                for compiler_name, command in cell['commands'].items():
                    self.assertEqual('3' if cell['mode'] == 'maximum' else '4', command[command.index('--checkers')+1])
                    self.assertEqual(compiler_name == 'tsrs' and cell['mode'] == 'maximum', '--maxMemory' in command)
            self.assertEqual(metadata, json.loads((plan / 'metadata.json').read_text()))
            # Repeating the script creates another complete bundle, without modifying the first.
            subprocess.run(['bash', str(runner)], env=env, capture_output=True, check=True)
            self.assertEqual(2, len(list((work / 'results').iterdir())))


@unittest.skipUnless(importlib.util.find_spec('matplotlib'), 'install bench/chart-assets/requirements.txt for rendering checks')
class BlogTests(unittest.TestCase):
    def test_relocated_archive_renders_identically_in_fresh_processes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'different-name'
            shutil.copytree(ARCHIVE, source, ignore=shutil.ignore_patterns('charts'))
            outputs = [root / 'first', root / 'second']
            env = dict(os.environ, MPLCONFIGDIR=os.environ.get('MPLCONFIGDIR', str(root / 'mpl')))
            for input_path, output_path in zip((ARCHIVE, source), outputs):
                subprocess.run([sys.executable, str(laptop.BENCH / 'chart.py'), str(input_path), '--format', 'blog',
                                '--output-dir', str(output_path)], env=env, capture_output=True, check=True)
            self.assertEqual(9, len(list(outputs[0].iterdir())))
            for path in outputs[0].iterdir():
                self.assertEqual(path.read_bytes(), (outputs[1] / path.name).read_bytes(), path.name)
            manifest = json.loads((outputs[0] / 'chart-manifest.json').read_text())
            self.assertEqual(240, manifest['samples'])
            self.assertEqual('0.18.0-ts7.1.0-dev.20260929', manifest['display_versions']['tsrs'])
            self.assertEqual('0.17.0-ts7.1.0-dev.20260929', manifest['measured_versions']['tsrs'])
            self.assertEqual((ARCHIVE / 'charts/means.json').read_bytes(), (outputs[0] / 'means.json').read_bytes())
            subprocess.run([sys.executable, str(laptop.BENCH / 'chart.py'), str(source), '--format', 'blog',
                            '--output-dir', str(outputs[0]), '--check'], env=env, capture_output=True, check=True)
            (outputs[0] / 'aligned.svg').write_text('stale')
            bad = subprocess.run([sys.executable, str(laptop.BENCH / 'chart.py'), str(source), '--format', 'blog',
                                  '--output-dir', str(outputs[0]), '--check'], env=env, capture_output=True, text=True)
            self.assertNotEqual(0, bad.returncode)
            self.assertIn('Charts need regeneration: aligned.svg', bad.stderr)


if __name__ == '__main__':
    unittest.main()
