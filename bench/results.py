"""Read and validate Hyperfine bundles without plotting dependencies or installed compilers."""
import csv
import json
import math
import shlex
from pathlib import Path

FIELDS = ("mode", "project", "compiler", "run", "wall_seconds", "instructions", "peak_rss_bytes", "exit_code")
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
    expected = {(m, p) for m in meta["modes"] for p in projects}
    cells = {(c["mode"], c["project"]): c for c in meta["cells"]}
    require(set(cells) == expected and len(cells) == len(meta["cells"]), "Missing or duplicate command cells")
    rows, preflights = [], {}
    for (mode, project), cell in cells.items():
        folder = source / mode / project
        checks = read_json(folder / "preflight.json")
        require(set(checks["compilers"]) == names, f"Incomplete preflight: {folder}")
        require(set(cell["commands"]) == names, f"Incomplete commands: {folder}")
        preflights[mode, project] = checks
        for result in measurements(folder / "hyperfine.json", meta["runs"], names):
            compiler = result["name"]
            require(shlex.split(result["command"]) == cell["commands"][compiler],
                    f"Measured command differs from metadata: {folder}/{compiler}")
            check = checks["compilers"][compiler]
            require(check["files"] > 0, f"Empty preflight: {folder}/{compiler}")
            for index, sample in enumerate(result["measurements"], 1):
                require(sample["exit_code"] == check["exit_code"], f"Exit status changed: {folder}/{compiler}")
                rows.append(dict(zip(FIELDS, (mode, project, compiler, index,
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
    require(meta.get("completed_utc"), "Benchmark must be complete before rendering")
    rows, preflights = collect_samples(source, meta)
    require(read_json(source / "raw.json") == rows, "raw.json differs from Hyperfine exports")
    with (source / "raw.csv").open(newline="") as stream:
        csv_rows = list(csv.DictReader(stream))
    require(len(csv_rows) == len(rows), "raw.csv has missing or extra rows")
    for saved, row in zip(csv_rows, rows):
        require(set(saved) == set(FIELDS), "Unexpected raw.csv columns")
        require(all(saved[k] == row[k] if isinstance(row[k], str) else float(saved[k]) == row[k]
                    for k in FIELDS), "raw.csv differs from Hyperfine exports")
    return meta, rows, preflights
