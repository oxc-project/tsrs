"""Offline coverage with synthetic samples; no recorded benchmark data is committed."""
import contextlib
import io
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

import chart
import compilers
import results
import run


def fixture(root, versions=('7.1.0-dev.20261019.1', '7.0.2'), checkers=4, memory=None,
            project_paths=(('vscode', 'src'), ('webpack', '.'))):
    root = root.resolve()
    source = root / 'results'
    source.mkdir(parents=True)
    compiler = root / 'compiler $literal'
    compiler.write_text('#!/usr/bin/env python3\nprint("Files: 1")\n')
    compiler.chmod(0o755)
    hf = root / 'hyperfine $literal'
    hf.write_text('''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
args = sys.argv[1:]
get = lambda flag: args[args.index(flag) + 1]
path = Path(get('--export-json'))
path.parent.mkdir(parents=True, exist_ok=True)
names = [args[i+1:i+3] for i, a in enumerate(args) if a == '--command-name'] or [('probe', '/usr/bin/true')]
results = []
for index, (name, command) in enumerate(names, 1):
    sample = {'time_wall_clock': {'value': index * 0.125, 'unit': 'second'},
              'instructions': {'value': index * 12345},
              'memory_peak_resident': {'value': index * 2**29, 'unit': 'byte'}, 'exit_code': 0}
    results.append({'name': name, 'command': command, 'measurements': [sample] * int(get('--runs'))})
path.write_text(json.dumps({'schema_version': 2, 'results': results}))
if '--output' in args:
    Path(get('--output')).write_text('Compiler output\\n')
    (path.parent / 'observed.json').write_text(json.dumps({'argv': args, 'cwd': os.getcwd(),
        'env': {k:v for k,v in os.environ.items() if k.startswith(('GO', 'RAYON_', 'TSRS_'))}}))
print('STUB_HYPERFINE_FINISHED', flush=True)
''')
    hf.chmod(0o755)
    projects = []
    for name, config in project_paths:
        (root / 'solutions' / name / config).mkdir(parents=True)
        projects.append({'name': name, 'project': config, 'commit': 'synthetic', 'repo': 'synthetic'})
    packages = {f'typescript_{i}': {'label': 'TypeScript', 'version': v, 'requested': f'typescript@{v}',
                                  'package': '@typescript/typescript-darwin-arm64'} for i, v in enumerate(versions, 1)}
    packages['tsrs'] = {'label': 'tsrs', 'version': '0.18.0-ts7.1.0-dev.20260929',
                        'requested': '@tsrs/darwin-arm64@0.18.0-ts7.1.0-dev.20260929', 'package': '@tsrs/darwin-arm64'}
    executables = {c: compiler for c in packages}
    settings = {'checkers': checkers, 'workers': 3, 'memory_gib': memory}
    meta = {'schema_version': 1, 'machine': {'cpus': 3, 'cpu': 'Synthetic CPU', 'memory_gb': 8,
                                           'platform': 'Synthetic OS', 'label': 'Synthetic machine'},
            'projects': projects, 'settings': settings, 'runs': 2, 'warmup': 1, 'compilers': packages,
            'hyperfine': {'version': 'hyperfine 2.0.0'},
            'cells': compilers.plan_cells({}, projects, settings, executables, root, hf, 2, 1, source)}
    # Write exports without timing anything, using the fake Hyperfine's deterministic samples.
    for cell in meta['cells']:
        directory = source / cell['project']
        directory.mkdir()
        checks = {c: {'files': 1, 'errors': 0, 'error_keys': {}, 'exit_code': 0} for c in packages}
        compilers.write_json(directory / 'preflight.json', {'compilers': checks})
        subprocess.run(cell['hyperfine_argv'], cwd=cell['cwd'], check=True, capture_output=True)
    rows, _ = results.collect_samples(source, meta)
    results.write_samples(source, rows)
    meta.update(started_utc='2026-01-01T00:00:00Z', completed_utc='2026-01-01T00:01:00Z')
    compilers.write_json(source / 'metadata.json', meta)
    return source, meta, hf


class BenchmarkTests(unittest.TestCase):
    def test_cli_passes_versions_repos_and_resources_to_one_plan(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            _, meta, hf = fixture(root)
            executables = {c: root / 'compiler $literal' for c in meta['compilers']}
            stdout = io.StringIO()
            with patch.object(run, 'setup_project') as setup, \
                    patch.object(run, 'machine_info', return_value=meta['machine']), \
                    patch.object(compilers, 'hyperfine', return_value=hf), \
                    patch.object(compilers, 'output', return_value='hyperfine 2.0.0'), \
                    patch.object(compilers, 'install_compilers', return_value=(executables, meta['compilers'])) as install, \
                    contextlib.redirect_stdout(stdout):
                run.main(['--typescript-versions', 'next,7.0.2', '--tsrs-version', '0.22.0',
                          '--repos', 'webpack', 'vscode', '--checkers', '6', '--workers', '3', '--memory-gib', '4',
                          '--runs', '2', '--warmup', '0', '--work-dir', str(root), '--out-dir', str(root / 'out'), '--dry-run'])
            self.assertEqual(['next', '7.0.2'], install.call_args.args[2])
            self.assertEqual('0.22.0', install.call_args.args[3])
            self.assertEqual(2, setup.call_count)
            source, = (root / 'out').iterdir()
            plan = json.loads((source / 'metadata.json').read_text())
            self.assertEqual({'checkers': 6, 'workers': 3, 'memory_gib': 4}, plan['settings'])
            self.assertEqual(['webpack', 'vscode'], [c['project'] for c in plan['cells']])
            self.assertEqual(2, stdout.getvalue().count('--export-json'))
            self.assertNotIn('STUB_HYPERFINE_FINISHED', stdout.getvalue())
            self.assertNotIn('completed_utc', plan)

    def test_versions_are_resolved_independently_and_tsrs_version_is_honored(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = root / 'output'
            output.mkdir()
            requested = []

            def release(name, version):
                requested.append((name, version))
                resolved = '7.2.0-dev.123' if version == 'next' else version
                return {'name': name, 'version': resolved, 'dist': {'integrity': 'fixture'},
                        'optionalDependencies': {'@typescript/typescript-darwin-arm64': resolved}}

            def npm(argv, **kwargs):
                install = Path(argv[argv.index('--prefix') + 1])
                dependencies = {}
                for spec in argv[argv.index('--save-exact') + 1:]:
                    name, _, version = spec.rpartition('@')
                    dependencies[name] = version
                    package = install / 'node_modules' / name
                    package.mkdir(parents=True, exist_ok=True)
                    compilers.write_json(package / 'package.json', {'version': version})
                    binary = package / ('tsrs' if name.startswith('@tsrs/') else 'lib/tsc')
                    binary.parent.mkdir(parents=True, exist_ok=True)
                    binary.write_bytes(b'fixture')
                compilers.write_json(install / 'package-lock.json', {'packages': {'': {'dependencies': dependencies}}})

            with patch.object(run, 'native_platform', return_value='darwin-arm64'), \
                    patch.object(compilers, 'release', side_effect=release), \
                    patch.object(compilers.subprocess, 'run', side_effect=npm), \
                    patch.object(compilers, 'output', return_value='fixture version'):
                executables, packages = compilers.install_compilers(root, output, ['next', '7.0.2', '7.0.1'], '0.22.0')
            self.assertEqual(4, len(executables))
            self.assertEqual('0.22.0', packages['tsrs']['version'])
            self.assertEqual('7.2.0-dev.123', packages['typescript_1']['version'])
            self.assertIn(('@tsrs/darwin-arm64', '0.22.0'), requested)
            self.assertEqual(4, len(list(output.glob('*-package-lock.json'))))

    def test_one_configuration_printed_commands_and_environment(self):
        with tempfile.TemporaryDirectory(prefix='bench $literal ') as directory:
            source, meta, hf = fixture(Path(directory), checkers=14, memory=7)
            for path in source.iterdir():
                if path.is_dir():
                    shutil.rmtree(path)
            env = dict(os.environ, TSRS_UNWANTED='yes', GOGC='1', GOMEMLIMIT='1GiB', RAYON_NUM_THREADS='99')
            script = ('import json, pathlib, compilers; '
                      f'p=pathlib.Path({str(source)!r}); m=json.loads((p/"metadata.json").read_text()); '
                      'm.pop("completed_utc"); '
                      f'compilers.run_matrix(p,m,pathlib.Path({str(hf)!r}))')
            completed = subprocess.run([sys.executable, '-c', script], cwd=run.BENCH, env=env,
                                       capture_output=True, text=True, check=True)
            chunks = completed.stdout.split('STUB_HYPERFINE_FINISHED')
            self.assertEqual(4, len(chunks))  # Counter probe and two repositories.
            for chunk in chunks[:-1]:
                self.assertIn('--export-json', chunk)
            saved, rows, _ = results.load_bundle(source)
            self.assertEqual(12, len(rows))
            self.assertNotIn('modes', saved)
            self.assertTrue(all('mode' not in r for r in rows))
            for cell in saved['cells']:
                observed = json.loads((source / cell['project'] / 'observed.json').read_text())
                self.assertEqual(cell['cwd'], observed['cwd'])
                self.assertEqual(cell['hyperfine_argv'][1:], observed['argv'])
                self.assertNotIn('TSRS_UNWANTED', observed['env'])
                self.assertNotIn('GOGC', observed['env'])
                for key, value in cell['environment'].items():
                    self.assertEqual(value, observed['env'].get(key))
                for compiler, command in cell['commands'].items():
                    self.assertEqual('14', command[command.index('--checkers') + 1])
                    self.assertEqual(compiler == 'tsrs', '--maxMemory' in command)
                # The printed Bash command can be copied verbatim, including unusual path characters.
                subprocess.run(['bash', '-c', compilers.bash_command(cell['hyperfine_argv'], cell['cwd'], saved['settings'])],
                               env=env, capture_output=True, check=True)

    def test_cli_rejects_invalid_choices_before_setup(self):
        for args in (['--checkers', '0'], ['--memory-gib', '0'], ['--workers', '0'], ['--runs', '0'],
                     ['--warmup', '-1'], ['--projects', 'missing'], ['--projects', 'webpack,webpack'],
                     ['--typescript-versions', 'next,next'], ['--tsrs', 'binary', '--tsrs-version', 'latest']):
            with self.subTest(args=args), patch.object(run, 'setup_project') as setup, contextlib.redirect_stderr(io.StringIO()):
                with self.assertRaises(SystemExit):
                    run.main(args)
                setup.assert_not_called()

    def test_corruption_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            source, _, _ = fixture(Path(directory))
            mutations = [('raw.json', lambda d: d[0].update(instructions=1)),
                         ('metadata.json', lambda d: d.pop('completed_utc')),
                         ('metadata.json', lambda d: d['cells'].append(d['cells'][0])),
                         ('vscode/hyperfine.json', lambda d: d['results'][0]['measurements'][0]
                          ['memory_peak_resident'].update(unit='KiB')),
                         ('vscode/hyperfine.json', lambda d: d['results'][0].update(command='wrong')),
                         ('vscode/hyperfine.json', lambda d: d['results'][0]['measurements'].pop())]
            for filename, mutate in mutations:
                with self.subTest(filename=filename):
                    path = source / filename
                    original = path.read_bytes()
                    data = json.loads(original)
                    mutate(data)
                    path.write_text(json.dumps(data))
                    with self.assertRaises(ValueError):
                        results.load_bundle(source)
                    path.write_bytes(original)

    def test_existing_chart_accepts_selected_version_and_arbitrary_checkers(self):
        with tempfile.TemporaryDirectory() as directory:
            source, _, _ = fixture(Path(directory), checkers=14)
            data = results.for_chart(source, '7.0.2')
            self.assertEqual('7.0.2', data['tsgo']['version'])
            self.assertIn('--checkers 14', chart.svg(data))
            self.assertIn('7.0.2', chart.per_project_svg(data, 'wall_s'))
            with self.assertRaises(ValueError):
                results.for_chart(source, 'does-not-exist')


@unittest.skipUnless(importlib.util.find_spec('matplotlib'), 'install bench/requirements-charts.txt')
class BarChartTests(unittest.TestCase):
    def test_long_repository_names_fit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source, _, _ = fixture(root, project_paths=(('next-packages-next', '.'), ('Compiler-Unions', '.')))
            subprocess.run([sys.executable, str(run.BENCH / 'chart_bars.py'), str(source)],
                           capture_output=True, text=True, check=True)

    def test_variable_compiler_counts_and_relocation(self):
        for versions in (('7.0.2',), ('7.1.0-dev.20261019.1', '7.0.2'), ('7.2.0', '7.1.0', '7.0.2', '7.0.1')):
            with self.subTest(versions=versions), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source, _, _ = fixture(root, versions)
                relocated = root / 'copied-source'
                shutil.copytree(source, relocated)
                destinations = [root / 'first', root / 'second']
                for src, dst in zip((source, relocated), destinations):
                    subprocess.run([sys.executable, str(run.BENCH / 'chart_bars.py'), str(src), '--output-dir', str(dst)],
                                   capture_output=True, text=True, check=True)
                for path in destinations[0].iterdir():
                    self.assertEqual(path.read_bytes(), (destinations[1] / path.name).read_bytes(), path.name)
                manifest = json.loads((destinations[0] / 'chart-manifest.json').read_text())
                self.assertEqual(len(versions) + 1, len(manifest['compiler_order']))
                self.assertEqual(f'typescript_{len(versions)}', manifest['baseline'])
                self.assertEqual('DejaVu Sans (Matplotlib default)', manifest['font'])
                subprocess.run([sys.executable, str(run.BENCH / 'chart_bars.py'), str(source), '--output-dir',
                                str(destinations[0]), '--check'], capture_output=True, text=True, check=True)


if __name__ == '__main__':
    unittest.main()
