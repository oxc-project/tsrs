"""Read and validate Hyperfine bundles without plotting dependencies or installed compilers."""
import csv
import json
import math
import shlex
import shutil
from pathlib import Path

FIELDS = ("project", "compiler", "run", "wall_seconds", "instructions", "peak_rss_bytes", "exit_code")
METRICS = {"wall_seconds": ("time_wall_clock", "second"), "instructions": ("instructions", None),
           "peak_rss_bytes": ("memory_peak_resident", "byte")}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def read_json(path):
    return json.loads(path.read_text())


def measurements(path, runs, names=None):
    data = read_json(path)
    require(data.get("schema_version") == 2, f"Unsupported Hyperfine schema: {path}")
    results = data["results"]
    if names is not None:
        require(len(results) == len(names) and {r["name"] for r in results} == set(names),
                f"Missing or duplicate compilers: {path}")
    for result in results:
        require(len(result["measurements"]) == runs, f"Missing runs: {path}")
        for sample in result["measurements"]:
            for key, unit in METRICS.values():
                metric = sample.get(key, {})
                value = metric.get("value", 0)
                require(isinstance(value, (float, int)) and math.isfinite(value) and value > 0,
                        f"Invalid {key}: {path}")
                require(metric.get("unit") == unit, f"Unexpected {key} unit: {path}")
                if key != "time_wall_clock":
                    require(int(value) == value, f"Nonintegral {key}: {path}")
            require(sample["exit_code"] in (0, 1, 2), f"Compiler failed: {path}")
    return results


def collect_samples(source, meta):
    """The exports are authoritative; raw.json/raw.csv are lossless, checked projections."""
    names = set(meta["compilers"])
    projects = [p["name"] for p in meta["projects"]]
    require(len(projects) == len(set(projects)) and projects, "Duplicate or missing projects")
    require(meta["runs"] > 0 and meta["warmup"] >= 0, "Invalid repetition counts")
    expected = set(projects)
    cells = {c["project"]: c for c in meta["cells"]}
    require(set(cells) == expected and len(cells) == len(meta["cells"]), "Missing or duplicate command cells")
    rows, preflights = [], {}
    for project, cell in cells.items():
        folder = source / project
        checks = read_json(folder / "preflight.json")
        require(set(checks["compilers"]) == names, f"Incomplete preflight: {folder}")
        require(set(cell["commands"]) == names, f"Incomplete commands: {folder}")
        preflights[project] = checks
        for result in measurements(folder / "hyperfine.json", meta["runs"], names):
            compiler = result["name"]
            require(shlex.split(result["command"]) == cell["commands"][compiler],
                    f"Measured command differs from metadata: {folder}/{compiler}")
            check = checks["compilers"][compiler]
            require(check["files"] > 0, f"Empty preflight: {folder}/{compiler}")
            for index, sample in enumerate(result["measurements"], 1):
                require(sample["exit_code"] == check["exit_code"], f"Exit status changed: {folder}/{compiler}")
                rows.append(dict(zip(FIELDS, (project, compiler, index,
                    sample["time_wall_clock"]["value"], int(sample["instructions"]["value"]),
                    int(sample["memory_peak_resident"]["value"]), sample["exit_code"]))))
    return rows, preflights


def write_samples(source, rows):
    (source / "raw.json").write_text(json.dumps(rows, indent=2) + "\n")
    with (source / "raw.csv").open("w", newline="") as stream:
        writer = csv.DictWriter(stream, fieldnames=FIELDS)
        writer.writeheader()
        writer.writerows(rows)


def load_bundle(source):
    source = Path(source)
    meta = read_json(source / "metadata.json")
    require(meta.get("schema_version") == 1, "Unsupported result schema; expected a bench/run.py result directory")
    require(meta.get("completed_utc"), "Benchmark must be complete before rendering")
    rows, preflights = collect_samples(source, meta)
    require(read_json(source / "raw.json") == rows, "raw.json differs from Hyperfine exports")
    with (source / "raw.csv").open(newline="") as stream:
        csv_rows = list(csv.DictReader(stream))
    require(len(csv_rows) == len(rows), "raw.csv has missing or extra rows")
    for saved, row in zip(csv_rows, rows):
        require(set(saved) == set(FIELDS), "Unexpected raw.csv columns")
        require(all(saved[k] == row[k] if isinstance(row[k], str) else
                    (float(saved[k]) if k == "wall_seconds" else int(saved[k])) == row[k]
                    for k in FIELDS), "raw.csv differs from Hyperfine exports")
    return meta, rows, preflights


def merge_bundles(sources, destination):
    """Combine project bundles without averaging samples; retain each job's original metadata."""
    sources, destination = [Path(p) for p in sources], Path(destination)
    require(bool(sources), "No benchmark bundles to publish")
    bundles = [load_bundle(source)[0] for source in sources]
    base = bundles[0]
    projects, cells, seen = [], [], set()
    identity = lambda compilers: {name: {k: v for k, v in info.items() if k not in ('binary', 'requested')}
                                  for name, info in compilers.items()}
    for meta in bundles:
        for key in ('settings', 'runs', 'warmup', 'suite', 'hyperfine', 'machine'):
            require(meta.get(key) == base.get(key), f"Cannot combine different {key}")
        require(identity(meta['compilers']) == identity(base['compilers']), "Cannot combine different compiler builds")
        for project in meta['projects']:
            require(project['name'] not in seen, f"Duplicate project: {project['name']}")
            seen.add(project['name'])
            projects.append(project)
        cells.extend(meta['cells'])
    destination.mkdir(parents=True, exist_ok=False)
    for index, (source, meta) in enumerate(zip(sources, bundles)):
        provenance = destination / 'partials' / str(index)
        provenance.mkdir(parents=True)
        for path in source.iterdir():
            if path.is_file():
                shutil.copy2(path, provenance / path.name)
                if index == 0 and (path.name.endswith('package-lock.json') or path.name == 'npm-packages.json'):
                    shutil.copy2(path, destination / path.name)
        for project in meta['projects']:
            shutil.copytree(source / project['name'], destination / project['name'])
    merged = dict(base, projects=projects, cells=cells,
                  started_utc=min(m['started_utc'] for m in bundles),
                  completed_utc=max(m['completed_utc'] for m in bundles),
                  partials=[f'partials/{i}/metadata.json' for i in range(len(bundles))])
    rows, _ = collect_samples(destination, merged)
    write_samples(destination, rows)
    (destination / 'metadata.json').write_text(json.dumps(merged, indent=2) + '\n')
    return destination


def for_chart(source, typescript_version=None):
    """Present a selected version to the existing scatter/per-project renderer without rewriting raw data."""
    from statistics import mean
    meta, rows, _ = load_bundle(source)
    candidates = [c for c in meta['compilers'] if c != 'tsrs']
    if typescript_version:
        candidates = [c for c in candidates if c == typescript_version or
                      meta['compilers'][c]['version'] == typescript_version]
    require(bool(candidates), 'Requested TypeScript version was not measured')
    compiler = candidates[0]
    checkers = meta['settings']['checkers']
    mode = f'checkers{checkers}'
    tsrs = dict(meta['compilers']['tsrs'], commit='not recorded', build='release')
    result = {'date': meta['started_utc'], 'machine': meta['machine'], 'tsrs': tsrs,
              'tsgo': meta['compilers'][compiler], 'modes': [mode], 'reps': meta['runs'],
              'chart_points': {checkers: mode}, 'projects': {},
              'project_labels': {p['name']: p['name'] for p in meta['projects']}}
    for project in meta['projects']:
        name = project['name']
        cell = {}
        for source_compiler, dest_compiler in ((compiler, 'tsgo'), ('tsrs', 'tsrs')):
            group = [r for r in rows if r['project'] == name and r['compiler'] == source_compiler]
            cell[dest_compiler] = {'ok_runs': len(group), 'runs': len(group),
                                  'wall_s': mean(r['wall_seconds'] for r in group),
                                  'peak_rss_bytes': mean(r['peak_rss_bytes'] for r in group)}
        result['projects'][name] = {mode: cell}
    return result
