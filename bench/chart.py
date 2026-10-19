#!/usr/bin/env python3
"""Draw the README's chart from a recorded benchmark result.

For wall-time/peak-memory bars across multiple releases, use bench/chart_bars.py.

A scatter of how many times faster tsrs is than tsgo (x) against how many times less peak memory it uses (y), both
linear. Each project is a line through one point per checker count: both compilers at --checkers 1, 2, 4 and 8 on
the same 8-vCPU machine, so every point compares equal thread counts with at most one checker per core. Up and to the
right is better.
"""
from __future__ import annotations

import argparse
import json
import math
from pathlib import Path
import xml.etree.ElementTree as ET

import run


# The per-project charts' compilers, drawn top to bottom; bun check only when the result measured it on every project.
COMPILERS = ("tsrs", "bun", "tsgo")
# The equal-thread modes by checker count (bench/run.py CHART_CHECKER_MODES), the README chart's points.
POINTS = run.CHART_CHECKER_MODES
# The mode a result must have for the charts to be drawn (bench/publish.py).
MODE = POINTS[4]
PANELS = (("wall_s", "Check time", "faster"), ("peak_rss_bytes", "Peak memory", "less memory"))
GIB = 2**30
FONT = "sans-serif"

# The README chart's projects (bench/projects.json names) and their legend names, in legend order.
SCATTER_PROJECTS = {"vscode": "vscode", "mui-docs": "mui-docs",
                    "t3code-server": "t3code-server", "supabase-studio": "supabase", "playwright": "playwright",
                    "typeorm": "typeorm", "date-fns": "date-fns"}
# The per-project charts' projects (bench/RESULTS.md): the README chart's and four more applications.
DETAIL_PROJECTS = {**SCATTER_PROJECTS, "nuxt": "nuxt", "xstate-main": "xstate", "formbricks-web": "formbricks",
                   "cal-diy": "cal-diy"}
# Both charts' colors, after oxc.rs's palette: beige or near-black cards, Oxc teal (aqua when dark) for tsrs, the
# TypeScript blue for tsgo, the pink of Bun's logo for bun check. The README chart's project colors avoid all three,
# so a color means one thing in every chart. The README and bench/RESULTS.md show the light or dark chart to match the reader's GitHub
# theme.
THEMES = {
    "light": {"background": "#f4f3ec", "border": "#dcdad0", "text": "#16171d", "muted": "#6f6876", "grid": "#e2e0d6",
              "axis": "#16171d", "dot": "#16171d", "tsrs": "#0d6a73", "tsgo": "#3178c6", "bun": "#c2255c",
              "projects": ("#6c3bff", "#e8590c", "#2f9e44", "#d4a017", "#867e8e", "#a0522d", "#7b8a00")},
    "dark": {"background": "#16171d", "border": "#3b3440", "text": "#ffffff", "muted": "#a49cab", "grid": "#2a2630",
             "axis": "#867e8e", "dot": "#0c0912", "tsrs": "#32f3e9", "tsgo": "#58a6ff", "bun": "#f06595",
             "projects": ("#b39aff", "#ff922b", "#51cf66", "#fcc419", "#b0a8b9", "#e0a07a", "#c0cf3a")},
}
WIDTH, HEIGHT = 1200, 884
# The plot area's edges; its top moves down a legend row for each row the legend wraps onto.
PLOT_LEFT, PLOT_RIGHT, PLOT_TOP, PLOT_BOTTOM = 130, 1140, 205, 720
# The legend's first row, the pitch between rows and the gap between entries.
LEGEND_Y, LEGEND_ROW, LEGEND_GAP = 139, 34, 36


def measured(cell: dict, compiler: str, metric: str) -> float | None:
    sample = cell.get(compiler) or {}
    value = sample.get(metric)
    return value if (sample.get("ok_runs") and isinstance(value, (int, float))
                     and math.isfinite(value) and value > 0) else None


def format_value(metric: str, value: float | None) -> str:
    if value is None:
        return "n/a"
    return run.fmt_s(value) if metric == "wall_s" else f"{value / GIB:.2f} GiB"


def chart_mode(result: dict) -> str:
    """The per-project charts' mode: the most equal checker threads every one of their projects in the result has a
    tsgo and tsrs measurement at."""
    detail_projects = result.get("project_labels", DETAIL_PROJECTS)
    points = result.get("chart_points", POINTS)
    projects = [p for name, p in result["projects"].items() if name in detail_projects]
    for checkers in sorted(points, reverse=True):
        mode = points[checkers]
        if projects and all(measured(p.get(mode) or {}, c, m) is not None for p in projects
                            for c in ("tsgo", "tsrs") for m, *_ in PANELS):
            return mode
    raise ValueError(f"no results for {', '.join(detail_projects)} with every project measured in one of "
                     f"{', '.join(points.values())}")


def element(parent, tag, **attrs):
    return ET.SubElement(parent, tag, {key.replace('_', '-'): str(value) for key, value in attrs.items()})


def text(parent, x, y, value, size, weight=400, fill=None, anchor=None):
    attrs = {"x": x, "y": y, "font_size": size, "font_weight": weight}
    if fill:
        attrs["fill"] = fill
    if anchor:
        attrs["text_anchor"] = anchor
    element(parent, "text", **attrs).text = value


def ratios(cell: dict) -> tuple[float, float] | None:
    """tsgo's wall time and peak memory over tsrs's, or None when either compiler lacks a measurement."""
    values = {(c, m): measured(cell, c, m) for c in ("tsgo", "tsrs") for m in ("wall_s", "peak_rss_bytes")}
    if None in values.values():
        return None
    return (values["tsgo", "wall_s"] / values["tsrs", "wall_s"],
            values["tsgo", "peak_rss_bytes"] / values["tsrs", "peak_rss_bytes"])


def axis_end(high: float) -> tuple[float, float]:
    """A 0-based linear axis past `high` with at most 10 grid steps: its end and step."""
    step = nice_step(high * 8 / 10)
    end = math.ceil(high / step + 0.5) * step
    return end, step


def typescript_name(version: str) -> str:
    """The timed tsgo as the chart names it: `TypeScript 7.1 nightly` for a 7.1.0-dev build, else `TypeScript <version>`."""
    release, _, prerelease = version.partition("-")
    if prerelease.startswith("dev."):
        return f"TypeScript {'.'.join(release.split('.')[:2])} nightly"
    return f"TypeScript {version}"


def ink(fill: str) -> str:
    """Black or white, whichever reads better on `fill` (#rrggbb), by its relative luminance."""
    r, g, b = (int(fill[i:i + 2], 16) / 255 for i in (1, 3, 5))
    linear = [c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in (r, g, b)]
    return "#16171d" if 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2] > 0.18 else "#ffffff"


def machine_phrase(machine: dict) -> str:
    """`an 8-vCPU Linux machine`: the article follows the spoken number (an 8, an 11, an 18, a 16)."""
    cpus = machine.get("cpus")
    return (("an " if str(cpus).startswith(("8", "11", "18")) else "a ")
            + f"{cpus}-vCPU {machine.get('platform', 'Linux').split()[0]} machine")


def header(root, colors: dict, width: int, title: str, kind: str, subtitle: str):
    """Both charts' card, title, right-aligned kind and subtitle; returns the drawing group."""
    height = int(root.get("height"))
    element(root, "rect", x=0.5, y=0.5, width=width - 1, height=height - 1, rx=4, fill=colors["background"],
            stroke=colors["border"])
    drawing = element(root, "g", font_family=FONT, fill=colors["text"])
    text(drawing, 48, 66, title, 26, 600)
    text(drawing, width - 48, 66, kind, 21, fill=colors["muted"], anchor="end")
    text(drawing, 48, 98, subtitle, 17, fill=colors["muted"])
    return drawing


def footer(drawing, colors: dict, height: int, result: dict, modes: list[str], machine: dict) -> None:
    """Both charts' two footer lines: the compilers, then the machine and repetitions."""
    build = ("published npm binary" if result["tsrs"].get("package") else "local binary" if result.get("chart_points") else
             run.TSRS_BUILDS[result["tsrs"].get("build", "release")].replace("`", ""))
    text(drawing, 48, height - 50, f"tsgo: typescript@{result['tsgo']['version']}. tsrs: "
         f"{result['tsrs'].get('version') if result.get('chart_points') else result['tsrs']['commit'][:12]}, "
         f"{build}. Measured {result['date'][:10]}.", 14, fill=colors["muted"])
    text(drawing, 48, height - 28, f"{machine['label'].replace('`', '')}. Wall time and peak RSS, "
         f"{run.reps_text(result, modes)}.", 14, fill=colors["muted"])


def legend_entry(parent, colors: dict, x: float, y: float, fill: str, label: str) -> None:
    """A legend dot and its name, starting at `x`."""
    element(parent, "circle", cx=f"{x:.1f}", cy=y, r=9, fill=fill, stroke=colors["dot"], stroke_width=1.5)
    text(parent, f"{x + 20:.1f}", y + 7, label, 19, 500)


def text_width(value: str, size: float) -> float:
    """Roughly how wide `value` sets in the chart's sans-serif font."""
    return size * sum(0.3 if c in "ijlrtf-. " else 0.56 for c in value)


def svg(result: dict, theme: str = "light") -> str:
    project_names = result.get("project_labels", SCATTER_PROJECTS)
    chart_points = result.get("chart_points", POINTS)
    colors = THEMES[theme]
    series = {}
    for name in project_names:
        project = result["projects"].get(name) or {}
        points = [(checkers, mode, ratios(project[mode])) for checkers, mode in sorted(chart_points.items())
                  if mode in project and ratios(project[mode])]
        # A line joins only points measured on one machine: the 4-checker point's, or the first point's.
        machines = {mode: (project[mode].get("machine") or result["machine"])["label"] for _, mode, _ in points}
        anchor = machines.get(MODE) or next(iter(machines.values()), None)
        points = [point for point in points if machines[point[1]] == anchor]
        if points:
            series[name] = points
    if not series:
        raise ValueError(f"no tsgo and tsrs results for {', '.join(project_names)} in "
                         f"{', '.join(chart_points.values())}")
    modes = sorted({mode for points in series.values() for _, mode, _ in points},
                   key=lambda mode: next(c for c, m in chart_points.items() if m == mode))
    checker_counts = [c for c, m in sorted(chart_points.items()) if m in modes]
    machine = next((result["projects"][name][mode]["machine"] for name in series for mode in modes
                    if "machine" in (result["projects"][name].get(mode) or {})), result["machine"])
    cpus = machine_phrase(machine)
    counts = ", ".join(map(str, checker_counts[:-1])) + (" and " if len(checker_counts) > 1 else "") + \
        str(checker_counts[-1])

    x_end, x_step = axis_end(max(x for points in series.values() for *_, (x, _) in points))
    y_end, y_step = axis_end(max(y for points in series.values() for *_, (_, y) in points))
    x_of = lambda v: PLOT_LEFT + (PLOT_RIGHT - PLOT_LEFT) * v / x_end
    # The legend's rows, right-aligned under the subtitle: a dot and a name per project, wrapped to the card's width.
    rows: list[list[tuple[str, float]]] = [[]]
    for name in series:
        width = 20 + text_width(project_names[name], 19)
        if rows[-1] and sum(w + LEGEND_GAP for _, w in rows[-1]) + width > WIDTH - 96:
            rows.append([])
        rows[-1].append((name, width))
    plot_top = PLOT_TOP + LEGEND_ROW * (len(rows) - 1)
    y_of = lambda v: PLOT_BOTTOM - (PLOT_BOTTOM - plot_top) * v / y_end

    typescript = typescript_name(result["tsgo"]["version"])
    title = f"tsrs vs {typescript}"
    root = ET.Element("svg", {"xmlns": "http://www.w3.org/2000/svg", "width": str(WIDTH), "height": str(HEIGHT),
                              "viewBox": f"0 0 {WIDTH} {HEIGHT}", "role": "img", "aria-labelledby": "title description"})
    ET.SubElement(root, "title", id="title").text = title
    details = []
    for name, points in series.items():
        parts = []
        for checkers, mode, (speed, memory) in points:
            cell = result["projects"][name][mode]
            parts.append(f"--checkers {checkers}: {speed:.1f}x faster, {memory:.1f}x less memory (tsgo "
                         f"{format_value('wall_s', measured(cell, 'tsgo', 'wall_s'))}, "
                         f"{format_value('peak_rss_bytes', measured(cell, 'tsgo', 'peak_rss_bytes'))}; tsrs "
                         f"{format_value('wall_s', measured(cell, 'tsrs', 'wall_s'))}, "
                         f"{format_value('peak_rss_bytes', measured(cell, 'tsrs', 'peak_rss_bytes'))})")
        details.append(f"{project_names[name]}: {'; '.join(parts)}.")
    build = ("published npm binary" if result["tsrs"].get("package") else "local binary" if result.get("chart_points") else
             run.TSRS_BUILDS[result["tsrs"].get("build", "release")].replace("`", ""))
    ET.SubElement(root, "desc", id="description").text = (
        f"How many times faster tsrs type-checks than tsgo ({typescript}), against how many times less peak memory "
        f"it uses, both compilers at --checkers {counts} on {cpus}. Up and to the right is better. "
        + " ".join(details) + f" {run.reps_text(result, modes).capitalize()}. tsrs uses {build}. "
        f"Machine: {machine['label'].replace('`', '')}. Measured {result['date']}; tsrs {result['tsrs'].get('version', result['tsrs']['commit'])}. "
        f"TypeScript {result['tsgo']['version']}; TypeScript reference {(result.get('reference') or {}).get('commit', 'not recorded')}.")
    ET.SubElement(root, "metadata").text = (
        f"Generated by bench/chart.py; modes: {', '.join(modes)}; tsrs: {result['tsrs'].get('version', result['tsrs']['commit'])}; "
        f"measured: {result['date']}; build: {result['tsrs'].get('build', 'release')}. "
        "Details: https://github.com/oxc-project/tsrs/blob/main/bench/RESULTS.md")

    drawing = header(root, colors, WIDTH, title, "Speed vs peak memory",
                     f"Both compilers at the same checker count on {cpus}")

    # The key, right-aligned above the legend: the number in a dot is the run's parallel checker threads.
    key = element(drawing, "g", data_role="key")
    key_text = f"parallel checker threads (--checkers {', '.join(map(str, checker_counts))})"
    x = WIDTH - 48 - text_width(key_text, 17)
    text(key, f"{x:.1f}", 98, key_text, 17, fill=colors["muted"])
    element(key, "circle", cx=f"{x - 22:.1f}", cy=92, r=13, fill=colors["background"], stroke=colors["text"],
            stroke_width=1.5)
    text(key, f"{x - 22:.1f}", 97, "n", 14, 700, colors["text"], "middle")

    project_colors = {name: colors["projects"][i % len(colors["projects"])]
                      for i, name in enumerate(project_names)}
    legend = element(drawing, "g", data_role="legend")
    for row, items in enumerate(rows):
        y = LEGEND_Y + row * LEGEND_ROW
        x = WIDTH - 48
        for name, width in reversed(items):
            x -= width
            legend_entry(legend, colors, x, y, project_colors[name], project_names[name])
            x -= LEGEND_GAP

    grid = element(drawing, "g", data_role="grid")
    for k in range(int(round(y_end / y_step)) + 1):
        value = k * y_step
        y = y_of(value)
        if k:
            element(grid, "line", x1=PLOT_LEFT, y1=f"{y:.1f}", x2=PLOT_RIGHT, y2=f"{y:.1f}", stroke=colors["grid"],
                    stroke_width=1.5)
        text(grid, PLOT_LEFT - 18, f"{y + 7:.1f}", f"{value:g}x", 19, fill=colors["muted"], anchor="end")
    for k in range(int(round(x_end / x_step)) + 1):
        value = k * x_step
        x = x_of(value)
        element(grid, "line", x1=f"{x:.1f}", y1=PLOT_BOTTOM, x2=f"{x:.1f}", y2=PLOT_BOTTOM + 8, stroke=colors["axis"],
                stroke_width=1.5)
        text(grid, f"{x:.1f}", PLOT_BOTTOM + 36, f"{value:g}x", 19, fill=colors["muted"], anchor="middle")
    element(grid, "path", d=f"M{PLOT_LEFT} {plot_top}V{PLOT_BOTTOM}H{PLOT_RIGHT}", fill="none",
            stroke=colors["axis"], stroke_width=1.5)
    text(grid, (PLOT_LEFT + PLOT_RIGHT) / 2, PLOT_BOTTOM + 84, f"Times faster than {typescript}", 21,
         anchor="middle")
    text(grid, 0, 0, f"Times less peak memory than {typescript}", 21, anchor="middle")
    grid[-1].set("transform", f"translate({PLOT_LEFT - 78} {(plot_top + PLOT_BOTTOM) / 2}) rotate(-90)")

    # Lines first, then every dot above every line.
    plot = element(drawing, "g", data_role="points")
    for name, points in series.items():
        element(plot, "polyline", data_project=name, fill="none", stroke=project_colors[name], stroke_width=3.5,
                stroke_linejoin="round", stroke_linecap="round",
                points=" ".join(f"{x_of(s):.1f},{y_of(m):.1f}" for *_, (s, m) in points))
    # Each dot carries its checker count.
    for name, points in series.items():
        group = element(plot, "g", data_project=name)
        for checkers, mode, (speed, memory) in points:
            x, y = x_of(speed), y_of(memory)
            element(group, "circle", data_checkers=checkers, data_speedup=f"{speed:.6g}", data_memory=f"{memory:.6g}",
                    cx=f"{x:.1f}", cy=f"{y:.1f}", r=13, fill=project_colors[name], stroke=colors["dot"],
                    stroke_width=1.5)
            text(group, f"{x:.1f}", f"{y + 5:.1f}", str(checkers), 14, 700, ink(project_colors[name]), "middle")

    footer(drawing, colors, HEIGHT, result, modes, machine)
    ET.indent(root, space="  ")
    return ET.tostring(root, encoding="unicode") + "\n"


# The per-project charts (bench/RESULTS.md): one row per project, a dot per compiler on a shared axis. Check time is
# linear, so the long checks read as long; peak memory is logarithmic, since bun check and tsrs trade wins there and
# would sit on top of each other on a linear axis.
DETAIL_TOP, DETAIL_PITCH, DETAIL_LEFT, DETAIL_RIGHT = 200, 40, 230, 890
DETAIL = {"wall_s": ("Check time", "faster", "linear"), "peak_rss_bytes": ("Peak memory", "less", "log")}
# The direction note under each per-project chart's axis, after a left-pointing arrow.
BETTER = {"wall_s": "lower is better", "peak_rss_bytes": "less is better"}


def nice_step(span: float) -> float:
    """A 1-2-5 step that gives at most 8 grid lines over `span`."""
    magnitude = 10 ** math.floor(math.log10(span / 8))
    return next(m * magnitude for m in (1, 2, 5, 10) if span / (m * magnitude) <= 8)


def per_project_svg(result: dict, metric: str, theme: str = "light") -> str:
    """One project per row, sorted by tsgo's check time: tsgo's, bun check's and tsrs's value as dots joined by a
    line, then how many times faster (or less memory) tsrs is than tsgo, and bun check's multiple on the rows where
    it beats tsrs."""
    project_names = result.get("project_labels", DETAIL_PROJECTS)
    colors = THEMES[theme]
    label, word, scale = DETAIL[metric]
    mode = chart_mode(result)
    checkers = next(c for c, m in result.get("chart_points", POINTS).items() if m == mode)
    cells = {name: project[mode] for name, project in result["projects"].items()
             if name in project_names and mode in project}
    compilers = [c for c in COMPILERS if all(measured(cell, c, m) is not None for cell in cells.values()
                                             for m, *_ in PANELS)]
    typescript = typescript_name(result["tsgo"]["version"])
    names = {"tsrs": "tsrs", "bun": "bun check", "tsgo": typescript}
    order = sorted(cells, key=lambda name: -measured(cells[name], "tsgo", "wall_s"))
    unit = 1 if metric == "wall_s" else GIB
    values = {name: {c: measured(cells[name], c, metric) / unit for c in compilers} for name in order}
    low = min(v for row in values.values() for v in row.values())
    high = max(v for row in values.values() for v in row.values())
    if scale == "linear":
        step = nice_step(high)
        end = math.ceil(high / step) * step
        ticks = [k * step for k in range(int(round(end / step)) + 1)]
        x_of = lambda v: DETAIL_LEFT + (DETAIL_RIGHT - DETAIL_LEFT) * v / end
    else:
        steps = [m * 10 ** e for e in range(math.floor(math.log10(low)), math.ceil(math.log10(high)) + 1)
                 for m in (1, 2, 5)]
        start = max(t for t in steps if t <= low)
        end = min((t for t in steps if t >= high), default=10 ** math.ceil(math.log10(high)))
        ticks = [t for t in steps if start <= t <= end]
        x_of = lambda v: DETAIL_LEFT + (DETAIL_RIGHT - DETAIL_LEFT) * math.log(v / start) / math.log(end / start)
    bottom = DETAIL_TOP + len(order) * DETAIL_PITCH - DETAIL_PITCH / 2
    height = int(bottom + 150)
    machine = next((cell["machine"] for cell in cells.values() if "machine" in cell), result["machine"])
    threads = f"--checkers {checkers}" + (f", bun check at --threads {checkers}" if "bun" in compilers else "")
    title = f"tsrs vs {typescript}"

    root = ET.Element("svg", {"xmlns": "http://www.w3.org/2000/svg", "width": str(WIDTH), "height": str(height),
                              "viewBox": f"0 0 {WIDTH} {height}", "role": "img",
                              "aria-labelledby": "title description"})
    ET.SubElement(root, "title", id="title").text = f"{label} by project: {title}"
    ET.SubElement(root, "desc", id="description").text = " ".join(
        f"{project_names[name]}: " + ", ".join(f"{names[c]} {format_value(metric, values[name][c] * unit)}"
                                                for c in compilers) + "."
        for name in order) + f" Both compilers at {threads} on {machine_phrase(machine)}. Smaller is better."
    ET.SubElement(root, "metadata").text = (
        f"Generated by bench/chart.py; metric: {metric}; mode: {mode}; tsrs: {result['tsrs'].get('version', result['tsrs']['commit'])}; "
        f"measured: {result['date']}.")
    drawing = header(root, colors, WIDTH, title, f"{label} by project",
                     f"{'All three' if len(compilers) == 3 else 'Both'} compilers at {threads} on "
                     f"{machine_phrase(machine)}, sorted by tsgo's check time")
    legend = element(drawing, "g", data_role="legend")
    x = WIDTH - 48
    for compiler in reversed(compilers):
        x -= 20 + text_width(names[compiler], 19)
        legend_entry(legend, colors, x, 139, colors[compiler], names[compiler])
        x -= LEGEND_GAP

    grid = element(drawing, "g", data_scale=scale)
    for tick in ticks:
        x = x_of(tick)
        element(grid, "line", x1=f"{x:.1f}", y1=DETAIL_TOP - 22, x2=f"{x:.1f}", y2=bottom, stroke=colors["grid"],
                stroke_width=1.5)
        text(grid, f"{x:.1f}", bottom + 30, f"{tick:g}", 17, fill=colors["muted"], anchor="middle")
    text(grid, (DETAIL_LEFT + DETAIL_RIGHT) / 2, bottom + 62,
         ("seconds" if metric == "wall_s" else "GiB") + (", log scale" if scale == "log" else ""), 19,
         anchor="middle")
    # Lower is better on both charts: an arrow pointing back along the axis, under its left end.
    better = element(drawing, "g", data_role="better")
    y = bottom + 62
    element(better, "line", x1=DETAIL_LEFT + 4, y1=y - 6, x2=DETAIL_LEFT + 58, y2=y - 6, stroke=colors["text"],
            stroke_width=2.5, stroke_linecap="round")
    element(better, "path", d=f"M{DETAIL_LEFT - 4} {y - 6}l13 -8v16z", fill=colors["text"])
    text(better, DETAIL_LEFT + 70, y, BETTER[metric], 19, 600)

    for row, name in enumerate(order):
        y = DETAIL_TOP + row * DETAIL_PITCH
        v = values[name]
        line = element(drawing, "g", data_project=name)
        text(line, DETAIL_LEFT - 22, y + 6, project_names[name], 19, 500, anchor="end")
        xs = [x_of(v[c]) for c in compilers]
        element(line, "line", x1=f"{min(xs):.1f}", y1=y, x2=f"{max(xs):.1f}", y2=y, stroke=colors["grid"],
                stroke_width=6, stroke_linecap="round")
        for compiler in ("tsgo", "bun", "tsrs"):
            if compiler in compilers:
                element(line, "circle", data_compiler=compiler, cx=f"{x_of(v[compiler]):.1f}", cy=y, r=9,
                        fill=colors[compiler], stroke=colors["dot"], stroke_width=1.5)
        labels = [("tsrs", f"{v['tsgo'] / v['tsrs']:.1f}x {word}")]
        if "bun" in compilers and v["bun"] < v["tsrs"]:
            labels.append(("bun", f"bun {v['tsgo'] / v['bun']:.1f}x {word}"))
        x = max(xs) + 20
        for compiler, label_text in labels:
            text(line, f"{x:.1f}", y + 6, label_text, 17, 600, colors[compiler])
            x += text_width(label_text, 17) + 18

    footer(drawing, colors, height, result, [mode], machine)
    ET.indent(root, space="  ")
    return ET.tostring(root, encoding="unicode") + "\n"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("result", type=Path, help="recorded JSON or bench/run.py result directory")
    parser.add_argument("--output", type=Path, required=True, help="SVG destination")
    parser.add_argument("--typescript-version", help="version/ID to compare with tsrs (default: first TypeScript in the result)")
    parser.add_argument("--theme", choices=THEMES, default="light")
    parser.add_argument("--per-project", choices=DETAIL)
    args = parser.parse_args()
    try:
        if args.result.is_dir():
            from results import for_chart
            result = for_chart(args.result, args.typescript_version)
        else:
            if args.typescript_version:
                parser.error("--typescript-version applies to result directories")
            result = json.loads(args.result.read_text())
        image = per_project_svg(result, args.per_project, args.theme) if args.per_project else svg(result, args.theme)
    except (OSError, ValueError) as error:
        parser.error(str(error))
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(image)


if __name__ == "__main__":
    main()
