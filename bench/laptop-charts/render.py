#!/usr/bin/env python3
"""Render the TypeScript Default Config and Maximum resources charts from a laptop.sh result directory."""
import argparse
import csv
import hashlib
import html
import json
import math
import os
import statistics
from collections import defaultdict
from pathlib import Path
import xml.etree.ElementTree as ET

ASSETS = Path(__file__).resolve().parent
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("results", type=Path, help="completed result directory produced by bench/laptop.sh")
parser.add_argument("--output-dir", type=Path, help="default: <results>/charts")
parser.add_argument("--tsrs-display-version", help="override the tsrs legend version without changing recorded measurements")
args = parser.parse_args()
SOURCE = args.results.resolve()
HERE = args.output_dir.resolve() if args.output_dir else SOURCE / "charts"
for name in ("metadata.json", "raw.json"):
    if not (SOURCE / name).is_file():
        parser.error(f"Missing {name} in {SOURCE}")
HERE.mkdir(parents=True, exist_ok=True)
os.environ.setdefault("MPLCONFIGDIR", str(HERE / ".matplotlib"))
import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
from matplotlib.font_manager import FontProperties
from matplotlib.patches import Rectangle
from matplotlib.transforms import Affine2D

META = json.loads((SOURCE / "metadata.json").read_text())
RAW = json.loads((SOURCE / "raw.json").read_text())
PROJECTS = [p["name"] for p in META["projects"]]
COMPILERS = tuple(c for c in ("typescript_7_0_2", "typescript", "tsrs") if c in META["compilers"])
assert {"typescript", "tsrs"} <= set(COMPILERS) == set(META["compilers"])
MODES = ("aligned", "maximum")
TITLES = {"aligned": "TypeScript Default Config", "maximum": "Maximum resources"}
FOOTER_HEIGHT = .45
FIGURE_HEIGHT = 5.3 - .38 + FOOTER_HEIGHT
PNG_DIMENSIONS = [2200, round(FIGURE_HEIGHT * 200)]
COLORS = {"typescript_7_0_2": "#91B8E3", "typescript": "#3178C6", "tsrs": "#F38020"}
NAMES = {c: META["compilers"][c].get("label", "tsrs" if c == "tsrs" else "TypeScript") for c in COMPILERS}
DISPLAY_VERSIONS = {c: META["compilers"][c]["version"] for c in COMPILERS}
if args.tsrs_display_version:
    DISPLAY_VERSIONS["tsrs"] = args.tsrs_display_version
THREE_COMPILERS = len(COMPILERS) == 3
INK, MUTED, GRID = "#24282C", "#555E68", "#E7EAEE"
LABELS = {"vscode": "VS Code", "mui-docs": "MUI docs", "webpack": "webpack", "t3code-server": "t3code server"}
GIB = 2**30
RUNS, WARMUPS = META["runs"], META["warmup"]
CPU, CORES = META["machine"]["cpu"], META["machine"]["cores"]
RAM = META["machine"]["ram_bytes"] / GIB
SOFT_MEMORY = META["modes"]["maximum"]["memory_gib"]
GROUPS, MEANS, PREFLIGHTS = defaultdict(list), {}, {}
assert META.get("completed_utc"), "Benchmark must be complete before rendering"
assert 1 <= len(PROJECTS) <= 4 and len(set(PROJECTS)) == len(PROJECTS)
assert len(RAW) == len(MODES) * len(PROJECTS) * len(COMPILERS) * RUNS
for row in RAW:
    GROUPS[row["mode"], row["project"], row["compiler"]].append(row)
for mode in MODES:
    for project in PROJECTS:
        hf = json.loads((SOURCE / mode / project / "hyperfine.json").read_text())
        assert len(hf["results"]) == len(COMPILERS) and {r["name"] for r in hf["results"]} == set(COMPILERS)
        preflight = json.loads((SOURCE / mode / project / "preflight.json").read_text())
        PREFLIGHTS[mode, project] = preflight
        for result in hf["results"]:
            key = mode, project, result["name"]
            group = sorted(GROUPS[key], key=lambda r: r["run"])
            assert [r["run"] for r in group] == list(range(1, RUNS + 1))
            assert len(result["measurements"]) == RUNS
            for row, sample in zip(group, result["measurements"]):
                assert row["wall_seconds"] == sample["time_wall_clock"]["value"]
                assert row["peak_rss_bytes"] == sample["memory_peak_resident"]["value"]
                assert all(math.isfinite(row[m]) and row[m] > 0 for m in ("wall_seconds", "peak_rss_bytes"))
            MEANS[key] = {"wall_seconds": statistics.mean(r["wall_seconds"] for r in group),
                          "peak_rss_gib": statistics.mean(r["peak_rss_bytes"] for r in group) / GIB}

FONTS = {style: FontProperties(fname=str(ASSETS / "fonts" / f"Inter-{name}.ttf"))
         for style, name in (("regular", "Regular"), ("medium", "Medium"), ("semibold", "SemiBold"))}
# Outline the text in SVG exports so the chosen font renders identically in the blog.
plt.rcParams.update({"font.size": 11, "svg.fonttype": "path", "text.color": INK})

def command_details(mode, compiler):
    """Read the measured command, substituting only the project-specific path."""
    configurations = set()
    for cell in META["cells"]:
        if cell["mode"] != mode:
            continue
        command = cell["commands"][compiler]
        args = command[1:].copy()
        args[args.index("-p") + 1] = "<project>"
        keys = ("RAYON_NUM_THREADS",) if compiler == "tsrs" else ("GOMAXPROCS", "GOGC", "GOMEMLIMIT")
        environment = tuple(f"{key}={cell['environment'][key]}" for key in keys
                            if cell["environment"].get(key) is not None)
        configurations.add((Path(command[0]).name, tuple(args), environment))
    assert len(configurations) == 1, f"Commands or resource settings differ across projects: {mode}/{compiler}"
    executable, args, environment = configurations.pop()
    return {"command": " ".join((executable, *args)), "arguments": list(args), "environment": list(environment)}

COMMANDS = {mode: {c: command_details(mode, c) for c in COMPILERS} for mode in MODES}

def shared_arguments(mode):
    # The harness appends compiler-specific resource flags after the shared arguments.
    common = []
    for values in zip(*(COMMANDS[mode][c]["arguments"] for c in COMPILERS)):
        if len(set(values)) != 1:
            break
        common.append(values[0])
    assert common[:3] == ["-p", "<project>", "--noEmit"]
    return " ".join(common)

SHARED_ARGUMENTS = {mode: shared_arguments(mode) for mode in MODES}

def comparison_label(metric, value, baseline, compiler):
    if compiler == "tsrs":
        return "(1× baseline)"
    ratio = value / baseline
    if metric == "wall_seconds":
        if value == baseline:
            return "(1× baseline)"
        return f"({ratio:.2f}× slower)" if ratio > 1 else f"({1 / ratio:.2f}× faster)"
    return f"({ratio:.2f}× memory)"

def alt_text(mode):
    settings = META["modes"][mode]
    palette = "TypeScript 7.0.2 in light blue; " if THREE_COMPILERS else ""
    parts = [f"{TITLES[mode]}: {palette}TypeScript nightly in blue; tsrs in orange.",
             f"Arithmetic means of {RUNS} runs after {WARMUPS} warmups, per project; {settings['checkers']} checker threads each."]
    if DISPLAY_VERSIONS["tsrs"] != META["compilers"]["tsrs"]["version"]:
        parts.append(f"The tsrs legend displays {DISPLAY_VERSIONS['tsrs']}; "
                     f"the saved measurements were recorded with {META['compilers']['tsrs']['version']}.")
    for project in PROJECTS:
        values = []
        for compiler in COMPILERS:
            mean = MEANS[mode, project, compiler]
            values.append(f"{NAMES[compiler]} {META['compilers'][compiler]['version']}: "
                          f"{mean['wall_seconds']:.3f} s and {mean['peak_rss_gib']:.3f} GiB")
        parts.append(f"{LABELS.get(project, project)}: " + "; ".join(values) + ".")
    parts.append("Each bar is compared with tsrs for the same project and mode, with tsrs marked as the 1× baseline. "
                 "Slower ratios divide the compiler's mean wall time by tsrs mean wall time; "
                 "memory ratios divide the compiler's mean peak RSS by tsrs mean peak RSS.")
    for compiler in COMPILERS:
        details = COMMANDS[mode][compiler]
        parts.append(f"{NAMES[compiler]} {META['compilers'][compiler]['version']}: {details['command']}; "
                     + " ".join(details["environment"]) + ".")
    return " ".join(parts)

def draw(mode):
    fig = plt.figure(figsize=(11, FIGURE_HEIGHT), facecolor="white")
    renderer = fig.canvas.get_renderer()
    # Preserve the plot's physical spacing, remove the subtitle's space and add a command footer.
    chart_transform = Affine2D().scale(1, 5.3 / FIGURE_HEIGHT).translate(0, FOOTER_HEIGHT / FIGURE_HEIGHT) + fig.transFigure

    def label(x, y, text, size=11, style="regular", color=INK, ha="left", va="baseline", transform=chart_transform):
        return fig.text(x, y, text, fontproperties=FONTS[style], fontsize=size, color=color, ha=ha, va=va, transform=transform)

    label(.055, .920 - .38 / 5.3, TITLES[mode], size=24, style="semibold")

    left = .055
    for compiler in COMPILERS:
        name, version = NAMES[compiler], DISPLAY_VERSIONS[compiler]
        fig.add_artist(Rectangle((left, .786), .013, .022, transform=chart_transform,
                                 facecolor=COLORS[compiler], edgecolor="none"))
        name_text = label(left + .023, .787, name, size=11.5, style="semibold")
        version_text = label(left + .023, .748, version, size=10, color=MUTED)
        # Pack the legends together, with a small gap after the wider of the name and version.
        left = max(t.get_window_extent(renderer).x1 for t in (name_text, version_text)) / fig.bbox.width + .030
    fig.add_artist(plt.Line2D([.055, .950], [.706, .706], transform=chart_transform, color=GRID, linewidth=.7))

    label(.055, .660, "Project", size=10.5, color=MUTED)
    columns = (("wall_seconds", .235, "Wall time", "s"),
               ("peak_rss_gib", .635, "Peak memory", "GiB"))
    scales = {}
    for metric, left, title, unit in columns:
        label(left, .660, title, size=13, style="semibold")
        # Keep a common zero-based scale across projects and both modes. Expand it
        # for future measurements when needed, reserving room for every value and annotation.
        scale = 8.0
        right = .60 if metric == "wall_seconds" else .97
        for other_mode in MODES:
            for project in PROJECTS:
                baseline = MEANS[other_mode, project, "tsrs"][metric]
                for compiler in COMPILERS:
                    value = MEANS[other_mode, project, compiler][metric]
                    texts = [label(0, 0, f"{value:.2f} {unit}", size=11.5, style="medium"),
                             label(0, 0, comparison_label(metric, value, baseline, compiler), size=10.5, style="medium")]
                    padding = .027
                    text_width = sum(t.get_window_extent(renderer).width for t in texts) / fig.bbox.width
                    for text in texts:
                        text.remove()
                    available = right - left - padding - text_width
                    assert available > 0, "Value labels are too long for the chart columns"
                    scale = max(scale, .300 * value / available)
        scales[metric] = scale

    offsets = (.028, -.010, -.048) if THREE_COMPILERS else (.008, -.037)
    bar_height = .024 if THREE_COMPILERS else .028
    for index, project in enumerate(PROJECTS):
        y = .535 - index * .138
        label(.055, y + statistics.mean(offsets), LABELS.get(project, project), size=11.5, style="semibold", va="center")
        if index:
            fig.add_artist(plt.Line2D([.055, .950], [y + .0545, y + .0545], transform=chart_transform,
                                     color="#ECEFF2", linewidth=.6))
        for metric, left, _, unit in columns:
            baseline = MEANS[mode, project, "tsrs"][metric]
            for compiler, offset in zip(COMPILERS, offsets):
                value = MEANS[mode, project, compiler][metric]
                width = .300 * value / scales[metric]
                fig.add_artist(Rectangle((left, y + offset - bar_height / 2), width, bar_height, transform=chart_transform,
                                         facecolor=COLORS[compiler], edgecolor="none"))
                value_text = label(left + width + .009, y + offset, f"{value:.2f} {unit}", size=11.5, style="medium", va="center")
                adjacent = value_text.get_window_extent(renderer).x1 / fig.bbox.width + .009
                note = label(adjacent, y + offset, comparison_label(metric, value, baseline, compiler),
                             size=10.5, style="medium", color="#9A4507" if compiler == "tsrs" else MUTED, va="center")
                assert note.get_window_extent(renderer).x1 / fig.bbox.width < (.60 if metric == "wall_seconds" else .97)

    fig.add_artist(plt.Line2D([.055, .950], [(FOOTER_HEIGHT + .10) / FIGURE_HEIGHT] * 2,
                             transform=fig.transFigure, color=GRID, linewidth=.7))
    heading = label(.055, .28 / FIGURE_HEIGHT, "All compilers:", size=10, style="semibold", transform=fig.transFigure)
    left = heading.get_window_extent(renderer).x1 / fig.bbox.width + .012
    text = label(left, .28 / FIGURE_HEIGHT, SHARED_ARGUMENTS[mode], size=10, color=MUTED, transform=fig.transFigure)
    assert text.get_window_extent(renderer).x1 / fig.bbox.width < .95, SHARED_ARGUMENTS[mode]

    # Validate that the full version strings, commands and data labels fit inside the artwork.
    fig.canvas.draw()
    bounds = fig.bbox
    for text in fig.texts:
        box = text.get_window_extent(fig.canvas.get_renderer())
        assert box.x0 >= 0 and box.y0 >= 0 and box.x1 <= bounds.width and box.y1 <= bounds.height, text.get_text()
    fig.savefig(HERE / f"{mode}.png", dpi=200, facecolor="white")
    fig.savefig(HERE / f"{mode}.svg", facecolor="white")
    plt.close(fig)
    path = HERE / f"{mode}.svg"
    svg = path.read_text()
    start = svg.index(">", svg.index("<svg")) + 1
    svg = svg[:start] + "\n<title>" + html.escape(TITLES[mode] + ": tsrs vs TypeScript") + "</title>\n<desc>" + html.escape(alt_text(mode)) + "</desc>" + svg[start:]
    path.write_text(svg)
    ET.fromstring(svg)

for mode in MODES:
    draw(mode)
mean_rows = [{"mode": mode, "project": project, "compiler": compiler, "measured_runs": RUNS,
              **MEANS[mode, project, compiler]} for mode in MODES for project in PROJECTS for compiler in COMPILERS]
(HERE / "means.json").write_text(json.dumps(mean_rows, indent=2) + "\n")
with (HERE / "means.csv").open("w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=list(mean_rows[0]))
    writer.writeheader()
    writer.writerows(mean_rows)
(HERE / "alt-text.txt").write_text("\n\n".join(alt_text(mode) for mode in MODES) + "\n")

scope = []
for (mode, project), preflight in PREFLIGHTS.items():
    counts = preflight["compilers"]
    details = []
    for compiler in COMPILERS:
        check = counts[compiler]
        description = f"{NAMES[compiler]} {META['compilers'][compiler]['version']}: {check['files']:,} files, {check['errors']:,} diagnostics"
        if compiler != "tsrs":
            match = "match" if check['error_keys'] == counts['tsrs']['error_keys'] else "differ"
            description += f"; diagnostic locations/codes {match} relative to tsrs"
        details.append(description)
    scope.append(f"{TITLES[mode]} / {LABELS.get(project, project)}: " + ". ".join(details) + ".")
facts = [
    ("Aggregation", f"Arithmetic mean of {RUNS} measured runs per compiler, project and mode. Warmups excluded. No aggregation across projects."),
    ("Mean peak memory", "Average the individual process peaks, then divide bytes by 2^30 for GiB."),
    ("Comparison labels", "tsrs is the 1× baseline for each project and mode. Slower ratio = compiler mean wall time / tsrs mean wall time; if a compiler is faster, show the reciprocal as × faster. Memory ratio = compiler mean peak RSS / tsrs mean peak RSS. Calculated before rounding."),
    ("Colors", "; ".join(f"{NAMES[c]} {META['compilers'][c]['version']}: {COLORS[c]}" for c in COMPILERS)),
    ("Machine", f"{CPU}; {CORES} logical CPUs; {RAM:g} GiB RAM; macOS {META['machine']['macos']}"),
    ("Compilers", "; ".join(f"{META['compilers'][c]['requested']}: {META['compilers'][c]['version']}" for c in COMPILERS) + "; native published binaries."),
    (TITLES["aligned"], f"{META['modes']['aligned']['checkers']} checkers each; default memory policies."),
    ("Maximum resources", f"{META['modes']['maximum']['checkers']} checkers each; tsrs --maxMemory {SOFT_MEMORY}G; Go GOMEMLIMIT={SOFT_MEMORY}GiB. Soft budgets, not hard RSS caps."),
    ("CPU and GC", f"Both modes: GOMAXPROCS={CORES}; RAYON_NUM_THREADS={CORES}; GOGC unset (default GC retained)."),
    ("Method", f"{META['hyperfine']['version']}; {WARMUPS} warmups and {RUNS} measurements; --shell=none; --noEmit --incremental false --pretty false. No timed --extendedDiagnostics."),
    ("Scope", " ".join(scope)),
    ("Source run", SOURCE.name),
    ("Command footer", "Shared arguments are shown once, read from saved commands for all compilers. <project> represents each row's project path. Compiler-specific flags and environment variables are retained below; GOGC is omitted when unset."),
]
for mode in MODES:
    facts.append((f"{TITLES[mode]} commands", "; ".join(
        f"{NAMES[c]} {META['compilers'][c]['version']}: "
        + " ".join((*COMMANDS[mode][c]["environment"], COMMANDS[mode][c]["command"])) for c in COMPILERS)))
if DISPLAY_VERSIONS["tsrs"] != META["compilers"]["tsrs"]["version"]:
    facts.append(("tsrs legend version", f"Display override: {DISPLAY_VERSIONS['tsrs']}. "
                  f"Measurements remain from {META['compilers']['tsrs']['version']}; the benchmark was not rerun."))
table = ''.join(f'<tr><th>{html.escape(k)}</th><td>{html.escape(v)}</td></tr>' for k, v in facts)
sections = []
for mode in MODES:
    svg = (HERE / f"{mode}.svg").read_text()
    svg = svg[svg.index("<svg"):]
    sections.append(f'<section id="{mode}"><div class="downloads"><strong>{TITLES[mode]}</strong>'
                    f'<a href="{mode}.png" download>PNG · {PNG_DIMENSIONS[0]} × {PNG_DIMENSIONS[1]}</a>'
                    f'<a href="{mode}.svg" download>SVG · vector</a></div>{svg}</section>')
page = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>tsrs vs TypeScript — blog charts</title><style>
*{box-sizing:border-box}body{margin:0;background:#f4f5f7;color:#24282c;font:16px/1.6 system-ui,sans-serif}
main{max-width:1280px;margin:auto;padding:32px}h1{font-size:28px;margin:0 0 8px}a{color:#2365a8}
nav{display:flex;gap:24px;flex-wrap:wrap;margin:18px 0}section,details{background:white;border:1px solid #e1e4e8;border-radius:8px;margin:24px 0;overflow:hidden}
svg{display:block;width:100%;height:auto}.downloads{padding:14px 24px;border-bottom:1px solid #e1e4e8}.downloads a{margin-left:20px}
details{padding:20px}summary{cursor:pointer;font-weight:650}table{border-collapse:collapse;width:100%;margin-top:18px;font-size:14px}
th,td{text-align:left;vertical-align:top;border-bottom:1px solid #e1e4e8;padding:10px}th{width:190px}
</style><main><h1>tsrs vs TypeScript</h1>
<p>Two blog figures: average wall time and mean peak memory per project, with comparison labels.</p>
<nav><a href="#aligned">TypeScript Default Config</a><a href="#maximum">Maximum resources</a><a href="#facts">Parameters</a>
<a href="means.csv" download>Chart data</a><a href="alt-text.txt" download>Alt text</a></nav>
'''
page += ''.join(sections)
page += '<details id="facts" open><summary>Parameters and calculation facts</summary><table>' + table + '</table></details>'
page += '<p>Color references: <a href="https://developers.cloudflare.com/workers/static-assets/direct-upload/">Cloudflare</a> · <a href="https://www.typescriptlang.org/branding/">TypeScript branding</a>.</p></main></html>'
(HERE / "index.html").write_text(page)
(HERE / "chart-manifest.json").write_text(json.dumps({
    "source": SOURCE.name, "raw_sha256": hashlib.sha256((SOURCE / "raw.json").read_bytes()).hexdigest(),
    "metadata_sha256": hashlib.sha256((SOURCE / "metadata.json").read_bytes()).hexdigest(),
    "samples": len(RAW), "bars_per_chart": len(PROJECTS) * len(COMPILERS) * 2,
    "aggregation": f"arithmetic mean of {RUNS} repetitions per compiler/project/mode",
    "cross_project_aggregation": False, "colors": {c: COLORS[c] for c in COMPILERS},
    "comparison_baseline": "tsrs", "matplotlib": matplotlib.__version__,
    "png_dimensions": PNG_DIMENSIONS, "font": "Inter", "svg_text": "outlined for consistent rendering",
    "titles": TITLES, "commands": COMMANDS, "shared_arguments": SHARED_ARGUMENTS,
    "display_versions": DISPLAY_VERSIONS,
    "measured_versions": {c: META["compilers"][c]["version"] for c in COMPILERS},
    "mean_values": mean_rows,
}, indent=2) + "\n")
print(f"Created two blog figures in {HERE}")
print(f"All {len(RAW)} source measurements validated against Hyperfine; {len(PROJECTS) * len(COMPILERS) * 2} bars in each figure.")
