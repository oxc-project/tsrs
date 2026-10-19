#!/usr/bin/env python3
"""Draw SVG wall-time and peak-memory bars from a completed bench/run.py result directory."""
import argparse
import csv
import hashlib
import html
import json
import math
import os
from pathlib import Path
import statistics
import textwrap
import xml.etree.ElementTree as ET

from results import load_bundle, require


class BarChart:
    def __init__(self, source, destination, baseline=None, title="TypeScript type checking"):
        self.source, self.here = Path(source).resolve(), Path(destination).resolve()
        self.here.mkdir(parents=True, exist_ok=True)
        self.meta, self.raw, self.preflights = load_bundle(self.source)
        self.projects = [p["name"] for p in self.meta["projects"]]
        self.compilers = ["tsrs", *[c for c in self.meta["compilers"] if c != "tsrs"]]
        self.names = {c: self.meta["compilers"][c]["label"] for c in self.compilers}
        self.versions = {c: self.meta["compilers"][c]["version"] for c in self.compilers}
        candidates = [c for c in self.compilers if c != "tsrs"]
        require(candidates and "tsrs" in self.meta["compilers"], "Select tsrs and at least one TypeScript version")
        if baseline is not None:
            candidates = [c for c in candidates if c == baseline or self.versions[c] == baseline]
            require(len(candidates) == 1, "--baseline must name one measured TypeScript version or compiler ID")
        self.baseline = candidates[-1]
        self.reference = f"TypeScript {self.versions[self.baseline]}"
        self.title = title
        self.figure_width, self.margin, self.bar_pitch = 12, .60, .24
        self.header_offset = (math.ceil(len(self.compilers) / 3) - 1) * .55
        self.first_row_y = 1.98 + self.header_offset + len(self.compilers) * self.bar_pitch / 2
        self.row_pitch = max(.86, len(self.compilers) * self.bar_pitch + .14)
        self.footer_rule_y = self.first_row_y + (len(self.projects) - .5) * self.row_pitch
        self.footer_y = self.footer_rule_y + .32
        self.figure_height = self.footer_y + .36
        # Cloudflare orange and TypeScript blue, with distinct blue shades for additional releases.
        blues = ("#3178C6", "#91B8E3", "#1D4F88", "#62A4D9", "#18405F", "#B5D0EB")
        self.colors = {"tsrs": "#F38020", **{c: blues[i % len(blues)] for i, c in enumerate(self.compilers[1:])}}
        self.ink, self.secondary, self.grid = "#18212B", "#36424E", "#E7EAEE"
        self.text_sizes = {"title": 24, "heading": 14, "legend": 13, "version": 11,
                           "project": 13, "value": 13, "comparison": 12, "footer": 12}
        self.text_weights = {role: "bold" if role in ("title", "heading", "legend", "project", "value") else "normal"
                             for role in self.text_sizes}
        self.labels = {"vscode": "VS Code", "mui-docs": "MUI docs", "webpack": "webpack", "t3code-server": "t3code server"}
        self.runs, self.warmups = self.meta["runs"], self.meta["warmup"]
        self.means = {}
        for project in self.projects:
            for compiler in self.compilers:
                group = [r for r in self.raw if r["project"] == project and r["compiler"] == compiler]
                self.means[project, compiler] = {"wall_seconds": statistics.mean(r["wall_seconds"] for r in group),
                                                "peak_rss_gib": statistics.mean(r["peak_rss_bytes"] for r in group) / 2**30}
        self.commands = {c: self.command_details(c) for c in self.compilers}
        common = []
        for values in zip(*(self.commands[c]["arguments"] for c in self.compilers)):
            if len(set(values)) != 1:
                break
            common.append(values[0])
        self.shared_arguments = " ".join(common)

    def command_details(self, compiler):
        configurations = set()
        for cell in self.meta["cells"]:
            command = cell["commands"][compiler]
            args = command[1:].copy()
            args[args.index("-p") + 1] = "<project>"
            keys = ("RAYON_NUM_THREADS",) if compiler == "tsrs" else ("GOMAXPROCS", "GOGC", "GOMEMLIMIT")
            environment = tuple(f"{key}={cell['environment'][key]}" for key in keys
                                if cell["environment"].get(key) is not None)
            configurations.add((Path(command[0]).name, tuple(args), environment))
        require(len(configurations) == 1, f"Resource settings differ across projects: {compiler}")
        executable, args, environment = configurations.pop()
        return {"command": " ".join((executable, *args)), "arguments": list(args), "environment": list(environment)}

    def comparison_label(self, metric, value, baseline, compiler):
        if compiler == self.baseline or value == baseline:
            return ""
        ratio = value / baseline
        if metric == "wall_seconds":
            return f"({ratio:.2f}× slower)" if ratio > 1 else f"({1 / ratio:.2f}× faster)"
        return f"({ratio:.2f}× memory)"

    def alt_text(self):
        parts = [f"{self.title}. Arithmetic means of {self.runs} runs after {self.warmups} warmups per project.",
                 f"Comparisons use {self.reference}. Wall time: reference/compiler; memory: compiler/reference."]
        for project in self.projects:
            for compiler in self.compilers:
                mean = self.means[project, compiler]
                parts.append(f"{self.labels.get(project, project)}, {self.names[compiler]} {self.versions[compiler]}: "
                             f"{mean['wall_seconds']:.3f} s; {mean['peak_rss_gib']:.3f} GiB.")
        parts.append("Settings: " + json.dumps(self.meta["settings"], sort_keys=True) + ".")
        return " ".join(parts)

    def draw(self):
        fig = plt.figure(figsize=(self.figure_width, self.figure_height), facecolor="white")
        renderer = fig.canvas.get_renderer()
        # Lay out in inches from the top left so every gap stays consistent as the figure grows.
        chart_transform = Affine2D().scale(1 / self.figure_width, -1 / self.figure_height).translate(0, 1) + fig.transFigure

        def label(x, y, text, role="comparison", color=self.ink):
            return fig.text(x, y, text, fontweight=self.text_weights[role], fontsize=self.text_sizes[role],
                            color=color, ha="left", va="center", transform=chart_transform)

        def text_right(text):
            return text.get_window_extent(renderer).x1 / fig.dpi

        def rule(y):
            fig.add_artist(plt.Line2D([self.margin, self.figure_width - self.margin], [y, y], transform=chart_transform,
                                     color=self.grid, linewidth=.7))

        label(self.margin, .43, self.title, role="title")
        label(self.margin, .72, f"Compared with {self.reference}", role="version", color=self.secondary)

        legend_width = (self.figure_width - 2 * self.margin) / min(3, len(self.compilers))
        for index, compiler in enumerate(self.compilers):
            left = self.margin + (index % 3) * legend_width
            legend_y = (index // 3) * .55
            name, version = self.names[compiler], self.versions[compiler]
            fig.add_artist(Rectangle((left, .97 + legend_y - .075), .15, .15, transform=chart_transform,
                                     facecolor=self.colors[compiler], edgecolor="none"))
            name_text = label(left + .25, .97 + legend_y, name, role="legend")
            version_text = label(left + .25, 1.23 + legend_y, version, role="version", color=self.secondary)
            assert max(text_right(t) for t in (name_text, version_text)) < left + legend_width - .15
        rule(1.52 + self.header_offset)

        label(self.margin, 1.78 + self.header_offset, "Project", role="heading")
        columns = (("wall_seconds", 2.30, 6.60, "Wall time", "s"),
                   ("peak_rss_gib", 7.10, 11.40, "Peak memory", "GiB"))
        scales = {}
        for metric, left, right, title, unit in columns:
            label(left, 1.78 + self.header_offset, title, role="heading")
            # Keep a common zero-based scale across projects. Expand it
            # for future measurements when needed, reserving room for every value and annotation.
            scale = 0
            for project in self.projects:
                baseline = self.means[project, self.baseline][metric]
                for compiler in self.compilers:
                    value = self.means[project, compiler][metric]
                    texts = [label(0, 0, f"{value:.2f} {unit}", role="value")]
                    comparison = self.comparison_label(metric, value, baseline, compiler)
                    padding = .20
                    if comparison:
                        texts.append(label(0, 0, comparison))
                        padding += .10
                    text_width = sum(t.get_window_extent(renderer).width for t in texts) / fig.dpi
                    for text in texts:
                        text.remove()
                    available = right - left - padding - text_width
                    assert available > 0, "Value labels are too long for the chart columns"
                    scale = max(scale, value / available)
            scales[metric] = scale

        offsets = [(i - (len(self.compilers) - 1) / 2) * self.bar_pitch for i in range(len(self.compilers))]
        bar_height = .14
        for index, project in enumerate(self.projects):
            y = self.first_row_y + index * self.row_pitch
            project_label = self.labels.get(project, project)
            name = label(self.margin, y, project_label, role="project")
            for width in range(14, 5, -1):
                if text_right(name) < columns[0][1] - .15:
                    break
                name.set_text(textwrap.fill(project_label, width=width))
            assert text_right(name) < columns[0][1] - .15
            if index:
                rule(y - self.row_pitch / 2)
            for metric, left, right, _, unit in columns:
                baseline = self.means[project, self.baseline][metric]
                for compiler, offset in zip(self.compilers, offsets):
                    value = self.means[project, compiler][metric]
                    width = value / scales[metric]
                    fig.add_artist(Rectangle((left, y + offset - bar_height / 2), width, bar_height, transform=chart_transform,
                                             facecolor=self.colors[compiler], edgecolor="none"))
                    value_text = label(left + width + .10, y + offset, f"{value:.2f} {unit}", role="value")
                    assert text_right(value_text) < right
                    comparison = self.comparison_label(metric, value, baseline, compiler)
                    if comparison:
                        note = label(text_right(value_text) + .10, y + offset, comparison, color=self.secondary)
                        assert text_right(note) < right

        rule(self.footer_rule_y)
        heading = label(self.margin, self.footer_y, "All compilers:", role="legend")
        text = label(text_right(heading) + .16, self.footer_y, self.shared_arguments, role="footer", color=self.secondary)
        assert text_right(text) < self.figure_width - self.margin, self.shared_arguments

        # Validate that the full version strings, commands and data labels fit inside the artwork.
        fig.canvas.draw()
        bounds = fig.bbox
        for text in fig.texts:
            box = text.get_window_extent(fig.canvas.get_renderer())
            assert box.x0 >= 0 and box.y0 >= 0 and box.x1 <= bounds.width and box.y1 <= bounds.height, text.get_text()
        fig.savefig(self.here / "benchmark.svg", facecolor="white", metadata={"Date": self.meta["completed_utc"]})
        plt.close(fig)
        path = self.here / "benchmark.svg"
        svg = path.read_text()
        start = svg.index(">", svg.index("<svg")) + 1
        svg = svg[:start] + "\n<title>" + html.escape(self.title + ": tsrs vs TypeScript") + "</title>\n<desc>" + html.escape(self.alt_text()) + "</desc>" + svg[start:]
        path.write_text(svg)
        ET.fromstring(svg)

    def render(self):
        self.draw()
        means = [{"project": p, "compiler": c, "measured_runs": self.runs, **self.means[p, c]}
                 for p in self.projects for c in self.compilers]
        (self.here / "means.json").write_text(json.dumps(means, indent=2) + "\n")
        with (self.here / "means.csv").open("w", newline="") as stream:
            writer = csv.DictWriter(stream, fieldnames=list(means[0]))
            writer.writeheader()
            writer.writerows(means)
        (self.here / "alt-text.txt").write_text(self.alt_text() + "\n")
        manifest = {"source": self.meta["started_utc"], "samples": len(self.raw),
                    "raw_sha256": hashlib.sha256((self.source / "raw.json").read_bytes()).hexdigest(),
                    "metadata_sha256": hashlib.sha256((self.source / "metadata.json").read_bytes()).hexdigest(),
                    "title": self.title, "versions": self.versions, "settings": self.meta["settings"],
                    "compiler_order": self.compilers, "baseline": self.baseline, "colors": self.colors,
                    "font": "DejaVu Sans (Matplotlib default)", "matplotlib": matplotlib.__version__,
                    "figure_size_inches": [self.figure_width, self.figure_height], "commands": self.commands,
                    "aggregation": "arithmetic mean per compiler/project; no cross-project aggregation",
                    "means": means}
        (self.here / "chart-manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        facts = [("Machine", self.meta["machine"]["label"]), ("Resource settings", json.dumps(self.meta["settings"])),
                 ("Method", f"{self.runs} measured runs after {self.warmups} warmups. Peak memory is the mean process peak RSS."),
                 ("Reference", self.reference),
                 ("Memory policy", "--memory-gib sets soft budgets through tsrs --maxMemory and Go GOMEMLIMIT; not hard RSS caps.")]
        for compiler in self.compilers:
            details = self.commands[compiler]
            facts.append((f"{self.names[compiler]} {self.versions[compiler]}",
                          " ".join([*details["environment"], details["command"]])))
        for project, check in self.preflights.items():
            facts.append((project, "; ".join(f"{self.names[c]} {self.versions[c]}: {v['files']} files, {v['errors']} diagnostics"
                                             for c, v in check["compilers"].items())))
        table = ''.join(f'<tr><th>{html.escape(k)}</th><td>{html.escape(v)}</td></tr>' for k, v in facts)
        page = ('<!doctype html><html lang="en"><meta charset="utf-8"><title>Compiler benchmark</title>'
                '<style>body{max-width:1200px;margin:32px auto;padding:0 20px;font:16px/1.6 sans-serif;color:#18212b}'
                'img{width:100%}table{border-collapse:collapse}th,td{text-align:left;vertical-align:top;padding:10px;'
                'border-bottom:1px solid #ddd}a{color:#3178c6}</style>'
                f'<h1>{html.escape(self.title)}</h1><img src="benchmark.svg" alt="{html.escape(self.alt_text(), quote=True)}">'
                '<p><a href="benchmark.svg">SVG</a> · '
                '<a href="means.csv">Chart data</a> · <a href="alt-text.txt">Alt text</a></p>'
                '<table>' + table + '</table></html>')
        (self.here / "index.html").write_text(page)
        print(f"Created chart in {self.here}; validated {len(self.raw)} raw samples.")


def render(source, destination, baseline=None, title="TypeScript type checking"):
    os.environ.setdefault("MPLCONFIGDIR", str(Path(destination).resolve().parent / ".matplotlib"))
    global matplotlib, plt, Rectangle, Affine2D
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.patches import Rectangle
    from matplotlib.transforms import Affine2D
    with matplotlib.rc_context():
        matplotlib.rcdefaults()
        plt.rcParams.update({"svg.fonttype": "path", "svg.hashsalt": "tsrs-benchmark-v1"})
        BarChart(source, destination, baseline, title).render()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("result", type=Path)
    parser.add_argument("--output-dir", type=Path, help="default: <result>/charts")
    parser.add_argument("--baseline", help="TypeScript version or compiler ID (default: last TypeScript in the run)")
    parser.add_argument("--title", default="TypeScript type checking")
    args = parser.parse_args()
    destination = args.output_dir or args.result / "charts"
    try:
        render(args.result, destination, args.baseline, args.title)
    except (OSError, ValueError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
