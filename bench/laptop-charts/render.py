#!/usr/bin/env python3
"""Render the Aligned and Maximum resources blog charts from a laptop.sh result directory."""
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

META = json.loads((SOURCE / "metadata.json").read_text())
RAW = json.loads((SOURCE / "raw.json").read_text())
PROJECTS = [p["name"] for p in META["projects"]]
COMPILERS, MODES = ("typescript", "tsrs"), ("aligned", "maximum")
COLORS = {"typescript": "#3178C6", "tsrs": "#F38020"}
INK, MUTED, GRID = "#24282C", "#555E68", "#E7EAEE"
LABELS = {"vscode": "VS Code", "mui-docs": "MUI docs", "webpack": "webpack", "t3code-server": "t3code server"}
GIB = 2**30
RUNS, WARMUPS = META["runs"], META["warmup"]
CPU, CORES = META["machine"]["cpu"], META["machine"]["cores"]
RAM = META["machine"]["ram_bytes"] / GIB
TS_VERSION = META["compilers"]["typescript"]["version"]
TSRS_VERSION = META["compilers"]["tsrs"]["version"]
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
        assert len(hf["results"]) == 2 and {r["name"] for r in hf["results"]} == set(COMPILERS)
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

def advantage_label(metric, ts, rs):
    if ts == rs:
        return "(same)"
    if metric == "wall_seconds":
        return f"({ts / rs:.2f}× faster)" if rs < ts else f"({rs / ts:.2f}× slower)"
    return f"({abs(1 - rs / ts) * 100:.1f}% {'less' if rs < ts else 'more'})"

def alt_text(mode):
    settings = META["modes"][mode]
    parts = [f"{settings['label']}: tsrs in orange; TypeScript nightly in blue.",
             f"Arithmetic means of {RUNS} runs after {WARMUPS} warmups, per project; {settings['checkers']} checker threads each."]
    for project in PROJECTS:
        ts, rs = (MEANS[mode, project, compiler] for compiler in COMPILERS)
        parts.append(f"{LABELS.get(project, project)}: TypeScript {ts['wall_seconds']:.3f} s and {ts['peak_rss_gib']:.3f} GiB; "
                     f"tsrs {rs['wall_seconds']:.3f} s and {rs['peak_rss_gib']:.3f} GiB.")
    return " ".join(parts)

def draw(mode):
    settings = META["modes"][mode]
    fig = plt.figure(figsize=(11, 5.3), facecolor="white")
    renderer = fig.canvas.get_renderer()

    def label(x, y, text, size=11, style="regular", color=INK, ha="left", va="baseline"):
        return fig.text(x, y, text, fontproperties=FONTS[style], fontsize=size, color=color, ha=ha, va=va)

    label(.055, .920, settings["label"], size=24, style="semibold")
    memory = "default memory policies" if mode == "aligned" else f"{SOFT_MEMORY} GiB soft memory budgets"
    label(.055, .859, f"{settings['checkers']} checkers  ·  {memory}", size=11.5, color=MUTED)

    left = .055
    for compiler, name, version in (("typescript", "TypeScript", TS_VERSION), ("tsrs", "tsrs", TSRS_VERSION)):
        fig.add_artist(Rectangle((left, .786), .013, .022, transform=fig.transFigure,
                                 facecolor=COLORS[compiler], edgecolor="none"))
        name_text = label(left + .023, .787, name, size=11.5, style="semibold")
        version_text = label(left + .023, .748, version, size=10, color=MUTED)
        # Pack the legends together, with a small gap after the wider of the name and version.
        left = max(t.get_window_extent(renderer).x1 for t in (name_text, version_text)) / fig.bbox.width + .030
    fig.add_artist(plt.Line2D([.055, .950], [.706, .706], transform=fig.transFigure, color=GRID, linewidth=.7))

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
                ts, rs = (MEANS[other_mode, project, c][metric] for c in COMPILERS)
                for compiler, value in (("typescript", ts), ("tsrs", rs)):
                    texts = [label(0, 0, f"{value:.2f} {unit}", size=11.5, style="medium")]
                    padding = .018
                    if compiler == "tsrs":
                        texts.append(label(0, 0, advantage_label(metric, ts, rs), size=10.5, style="medium"))
                        padding += .009
                    text_width = sum(t.get_window_extent(renderer).width for t in texts) / fig.bbox.width
                    for text in texts:
                        text.remove()
                    available = right - left - padding - text_width
                    assert available > 0, "Value labels are too long for the chart columns"
                    scale = max(scale, .300 * value / available)
        scales[metric] = scale

    for index, project in enumerate(PROJECTS):
        y = .535 - index * .138
        label(.055, y - .0145, LABELS.get(project, project), size=11.5, style="semibold", va="center")
        if index:
            fig.add_artist(plt.Line2D([.055, .950], [y + .0545, y + .0545], transform=fig.transFigure,
                                     color="#ECEFF2", linewidth=.6))
        for metric, left, _, unit in columns:
            ts, rs = (MEANS[mode, project, compiler][metric] for compiler in COMPILERS)
            advantage = advantage_label(metric, ts, rs)
            for compiler, offset in (("typescript", .008), ("tsrs", -.037)):
                value = MEANS[mode, project, compiler][metric]
                width = .300 * value / scales[metric]
                fig.add_artist(Rectangle((left, y + offset - .014), width, .028, transform=fig.transFigure,
                                         facecolor=COLORS[compiler], edgecolor="none"))
                value_text = label(left + width + .009, y + offset, f"{value:.2f} {unit}", size=11.5, style="medium", va="center")
                if compiler == "tsrs":
                    adjacent = value_text.get_window_extent(renderer).x1 / fig.bbox.width + .009
                    note = label(adjacent, y + offset, advantage, size=10.5, style="medium", color="#9A4507", va="center")
                    assert note.get_window_extent(renderer).x1 / fig.bbox.width < (.60 if metric == "wall_seconds" else .97)

    # Validate that the full version strings and data labels fit inside the artwork.
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
    svg = svg[:start] + "\n<title>" + html.escape(settings["label"] + ": tsrs vs TypeScript nightly") + "</title>\n<desc>" + html.escape(alt_text(mode)) + "</desc>" + svg[start:]
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
    match = "match" if preflight["comparison"]["same_diagnostic_keys"] else "differ"
    scope.append(f"{META['modes'][mode]['label']} / {LABELS.get(project, project)}: diagnostic locations/codes {match}; "
                 f"TypeScript {counts['typescript']['files']:,} files, tsrs {counts['tsrs']['files']:,} files.")
facts = [
    ("Aggregation", f"Arithmetic mean of {RUNS} measured runs per compiler, project and mode. Warmups excluded. No aggregation across projects."),
    ("Mean peak memory", "Average the individual process peaks, then divide bytes by 2^30 for GiB."),
    ("Comparison labels", "Speedup = TypeScript mean wall time / tsrs mean wall time. Memory reduction = 100 × (1 − tsrs mean peak RSS / TypeScript mean peak RSS). Calculated before rounding."),
    ("Colors", "tsrs: Cloudflare orange #F38020. TypeScript: #3178C6. Dark text labels for contrast."),
    ("Machine", f"{CPU}; {CORES} logical CPUs; {RAM:g} GiB RAM; macOS {META['machine']['macos']}"),
    ("Compilers", f"typescript@next: {TS_VERSION}; @tsrs/darwin-arm64@latest: {TSRS_VERSION}; native published binaries."),
    ("Aligned", f"{META['modes']['aligned']['checkers']} checkers each; default memory policies."),
    ("Maximum resources", f"{META['modes']['maximum']['checkers']} checkers each; tsrs --maxMemory {SOFT_MEMORY}G; Go GOMEMLIMIT={SOFT_MEMORY}GiB. Soft budgets, not hard RSS caps."),
    ("CPU and GC", f"Both modes: GOMAXPROCS={CORES}; RAYON_NUM_THREADS={CORES}; GOGC unset (default GC retained)."),
    ("Method", f"{META['hyperfine']['version']}; {WARMUPS} warmups and {RUNS} measurements; --shell=none; --noEmit --incremental false --pretty false. No timed --extendedDiagnostics."),
    ("Scope", " ".join(scope)),
    ("Source run", SOURCE.name),
]
table = ''.join(f'<tr><th>{html.escape(k)}</th><td>{html.escape(v)}</td></tr>' for k, v in facts)
sections = []
for mode in MODES:
    svg = (HERE / f"{mode}.svg").read_text()
    svg = svg[svg.index("<svg"):]
    sections.append(f'<section id="{mode}"><div class="downloads"><strong>{META["modes"][mode]["label"]}</strong>'
                    f'<a href="{mode}.png" download>PNG · 2200 × 1060</a><a href="{mode}.svg" download>SVG · vector</a></div>{svg}</section>')
page = '''<!doctype html><html lang="en"><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>tsrs vs TypeScript nightly — blog charts</title><style>
*{box-sizing:border-box}body{margin:0;background:#f4f5f7;color:#24282c;font:16px/1.6 system-ui,sans-serif}
main{max-width:1280px;margin:auto;padding:32px}h1{font-size:28px;margin:0 0 8px}a{color:#2365a8}
nav{display:flex;gap:24px;flex-wrap:wrap;margin:18px 0}section,details{background:white;border:1px solid #e1e4e8;border-radius:8px;margin:24px 0;overflow:hidden}
svg{display:block;width:100%;height:auto}.downloads{padding:14px 24px;border-bottom:1px solid #e1e4e8}.downloads a{margin-left:20px}
details{padding:20px}summary{cursor:pointer;font-weight:650}table{border-collapse:collapse;width:100%;margin-top:18px;font-size:14px}
th,td{text-align:left;vertical-align:top;border-bottom:1px solid #e1e4e8;padding:10px}th{width:190px}
</style><main><h1>tsrs vs TypeScript nightly</h1>
<p>Two blog figures: average wall time and mean peak memory per project, with comparison labels.</p>
<nav><a href="#aligned">Aligned</a><a href="#maximum">Maximum resources</a><a href="#facts">Parameters</a>
<a href="means.csv" download>Chart data</a><a href="alt-text.txt" download>Alt text</a></nav>
'''
page += ''.join(sections)
page += '<details id="facts" open><summary>Parameters and calculation facts</summary><table>' + table + '</table></details>'
page += '<p>Color references: <a href="https://developers.cloudflare.com/workers/static-assets/direct-upload/">Cloudflare</a> · <a href="https://www.typescriptlang.org/branding/">TypeScript branding</a>.</p></main></html>'
(HERE / "index.html").write_text(page)
(HERE / "chart-manifest.json").write_text(json.dumps({
    "source": SOURCE.name, "raw_sha256": hashlib.sha256((SOURCE / "raw.json").read_bytes()).hexdigest(),
    "metadata_sha256": hashlib.sha256((SOURCE / "metadata.json").read_bytes()).hexdigest(),
    "samples": len(RAW), "bars_per_chart": len(PROJECTS) * 4,
    "aggregation": f"arithmetic mean of {RUNS} repetitions per compiler/project/mode",
    "cross_project_aggregation": False, "colors": COLORS, "matplotlib": matplotlib.__version__,
    "png_dimensions": [2200, 1060], "font": "Inter", "svg_text": "outlined for consistent rendering",
    "mean_values": mean_rows,
}, indent=2) + "\n")
print(f"Created two blog figures in {HERE}")
print(f"All {len(RAW)} source measurements validated against Hyperfine; {len(PROJECTS) * 4} bars in each figure.")
