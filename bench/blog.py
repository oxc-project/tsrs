#!/usr/bin/env python3
"""Cloudflare blog figures from validated, saved Hyperfine results (bench/chart.py --format blog)."""
import csv
import hashlib
import html
import json
import os
import statistics
from collections import defaultdict
from pathlib import Path
import xml.etree.ElementTree as ET

from results import load_bundle, require

ASSETS = Path(__file__).resolve().parent / "chart-assets"


class BlogCharts:
    def __init__(self, source, destination, display_version=None):
        self.source, self.here = Path(source).resolve(), Path(destination).resolve()
        self.here.mkdir(parents=True, exist_ok=True)
        self.meta, self.raw, self.preflights = load_bundle(self.source)
        self.projects = [p["name"] for p in self.meta["projects"]]
        self.compilers = tuple(c for c in ("tsrs", "typescript", "typescript_7_0_2") if c in self.meta["compilers"])
        require({"typescript", "tsrs"} <= set(self.compilers) == set(self.meta["compilers"]),
                "Blog charts require tsrs, TypeScript nightly, and optionally TypeScript 7.0.2")
        self.modes = ("aligned", "maximum")
        require(set(self.meta["modes"]) == set(self.modes), "Blog charts require aligned and maximum modes")
        self.titles = {"aligned": "TypeScript Default Config", "maximum": "Maximum resources"}
        self.figure_width = 11
        self.margin = .60
        self.first_row_y, self.row_pitch, self.bar_pitch = 2.31, .86, .24
        self.footer_rule_y = self.first_row_y + (len(self.projects) - .5) * self.row_pitch
        self.footer_y = self.footer_rule_y + .32
        self.figure_height = self.footer_y + .36
        self.png_dimensions = [round(self.figure_width * 200), round(self.figure_height * 200)]
        self.colors = {"typescript_7_0_2": "#91B8E3", "typescript": "#3178C6", "tsrs": "#F38020"}
        self.names = {c: self.meta["compilers"][c].get("label", "tsrs" if c == "tsrs" else "TypeScript") for c in self.compilers}
        self.display_versions = {c: self.meta["compilers"][c]["version"] for c in self.compilers}
        settings_path = self.source / "chart.json"
        settings = json.loads(settings_path.read_text()) if settings_path.exists() else {}
        self.display_versions.update(settings.get("display_versions", {}))
        if display_version:
            self.display_versions["tsrs"] = display_version
        self.three_compilers = len(self.compilers) == 3
        self.baseline = "typescript_7_0_2" if self.three_compilers else "typescript"
        self.reference = f"{self.names[self.baseline]} {self.meta['compilers'][self.baseline]['version']}"
        self.ink, self.secondary, self.grid = "#18212B", "#36424E", "#E7EAEE"
        self.text_sizes = {"title": 24, "heading": 14, "legend": 13, "version": 12,
                           "project": 13, "value": 13, "comparison": 12, "footer": 12}
        self.text_weights = {"title": "medium", "heading": "medium", "legend": "medium", "version": "regular",
                             "project": "medium", "value": "medium", "comparison": "regular", "footer": "regular"}
        self.labels = {"vscode": "VS Code", "mui-docs": "MUI docs", "webpack": "webpack", "t3code-server": "t3code server"}
        self.gib = 2**30
        self.runs, self.warmups = self.meta["runs"], self.meta["warmup"]
        self.cpu, self.cores = self.meta["machine"]["cpu"], self.meta["machine"]["cores"]
        self.ram = self.meta["machine"]["ram_bytes"] / self.gib
        self.soft_memory = self.meta["modes"]["maximum"]["memory_gib"]
        self.groups, self.means = defaultdict(list), {}
        for row in self.raw:
            self.groups[row["mode"], row["project"], row["compiler"]].append(row)
        for key, group in self.groups.items():
            self.means[key] = {"wall_seconds": statistics.mean(r["wall_seconds"] for r in group),
                               "peak_rss_gib": statistics.mean(r["peak_rss_bytes"] for r in group) / self.gib}
        self.fonts = {style: FontProperties(fname=str(ASSETS / "fonts" / f"Inter-{name}.ttf"))
                      for style, name in (("regular", "Regular"), ("medium", "Medium"))}
        # Outline the text in SVG exports so the chosen font renders identically in the blog.
        plt.rcParams.update({"font.size": 11, "svg.fonttype": "path", "text.color": self.ink, "svg.hashsalt": "tsrs-benchmark-v1"})

        self.commands = {mode: {c: self.command_details(mode, c) for c in self.compilers} for mode in self.modes}

        self.shared_arguments = {mode: self.common_arguments(mode) for mode in self.modes}

    def command_details(self, mode, compiler):
        """Read the measured command, substituting only the project-specific path."""
        configurations = set()
        for cell in self.meta["cells"]:
            if cell["mode"] != mode:
                continue
            command = cell["commands"][compiler]
            args = command[1:].copy()
            args[args.index("-p") + 1] = "<project>"
            keys = ("RAYON_NUM_THREADS",) if compiler == "tsrs" else ("GOMAXPROCS", "GOGC", "GOMEMLIMIT")
            environment = tuple(f"{key}={cell['environment'][key]}" for key in keys
                                if cell["environment"].get(key) is not None)
            configurations.add((Path(command[0]).name, tuple(args), environment))
        require(len(configurations) == 1, f"Commands or resource settings differ across projects: {mode}/{compiler}")
        executable, args, environment = configurations.pop()
        return {"command": " ".join((executable, *args)), "arguments": list(args), "environment": list(environment)}

    def common_arguments(self, mode):
        # The harness appends compiler-specific resource flags after the shared arguments.
        common = []
        for values in zip(*(self.commands[mode][c]["arguments"] for c in self.compilers)):
            if len(set(values)) != 1:
                break
            common.append(values[0])
        require(common[:3] == ["-p", "<project>", "--noEmit"], "Unexpected shared compiler arguments")
        return " ".join(common)

    def comparison_label(self, metric, value, baseline, compiler):
        if compiler == self.baseline or value == baseline:
            return ""
        ratio = value / baseline
        if metric == "wall_seconds":
            return f"({ratio:.2f}× slower)" if ratio > 1 else f"({1 / ratio:.2f}× faster)"
        return f"({ratio:.2f}× memory)"

    def alt_text(self, mode):
        settings = self.meta["modes"][mode]
        palette = "TypeScript 7.0.2 in light blue; " if self.three_compilers else ""
        parts = [f"{self.titles[mode]}: {palette}TypeScript nightly in blue; tsrs in orange.",
                 f"Arithmetic means of {self.runs} runs after {self.warmups} warmups, per project; {settings['checkers']} checker threads each."]
        if self.display_versions["tsrs"] != self.meta["compilers"]["tsrs"]["version"]:
            parts.append(f"The tsrs legend displays {self.display_versions['tsrs']}; "
                         f"the saved measurements were recorded with {self.meta['compilers']['tsrs']['version']}.")
        for project in self.projects:
            values = []
            for compiler in self.compilers:
                mean = self.means[mode, project, compiler]
                values.append(f"{self.names[compiler]} {self.meta['compilers'][compiler]['version']}: "
                              f"{mean['wall_seconds']:.3f} s and {mean['peak_rss_gib']:.3f} GiB")
            parts.append(f"{self.labels.get(project, project)}: " + "; ".join(values) + ".")
        parts.append(f"Bars run from top to bottom: {', '.join(self.names[c] for c in self.compilers)}. "
                     f"Comparison labels are relative to {self.reference} for the same project and mode. "
                     "Faster ratios divide the reference mean wall time by the compiler's mean wall time; "
                     "memory ratios divide the compiler's mean peak RSS by the reference mean peak RSS. "
                     "The reference bars have no comparison labels.")
        for compiler in self.compilers:
            details = self.commands[mode][compiler]
            parts.append(f"{self.names[compiler]} {self.meta['compilers'][compiler]['version']}: {details['command']}; "
                         + " ".join(details["environment"]) + ".")
        return " ".join(parts)

    def draw(self, mode):
        fig = plt.figure(figsize=(self.figure_width, self.figure_height), facecolor="white")
        renderer = fig.canvas.get_renderer()
        # Lay out in inches from the top left so every gap stays consistent as the figure grows.
        chart_transform = Affine2D().scale(1 / self.figure_width, -1 / self.figure_height).translate(0, 1) + fig.transFigure

        def label(x, y, text, role="comparison", color=self.ink):
            return fig.text(x, y, text, fontproperties=self.fonts[self.text_weights[role]], fontsize=self.text_sizes[role],
                            color=color, ha="left", va="center", transform=chart_transform)

        def text_right(text):
            return text.get_window_extent(renderer).x1 / fig.dpi

        def rule(y):
            fig.add_artist(plt.Line2D([self.margin, self.figure_width - self.margin], [y, y], transform=chart_transform,
                                     color=self.grid, linewidth=.7))

        label(self.margin, .43, self.titles[mode], role="title")

        legend_width = (self.figure_width - 2 * self.margin) / len(self.compilers)
        for index, compiler in enumerate(self.compilers):
            left = self.margin + index * legend_width
            name, version = self.names[compiler], self.display_versions[compiler]
            fig.add_artist(Rectangle((left, .97 - .075), .15, .15, transform=chart_transform,
                                     facecolor=self.colors[compiler], edgecolor="none"))
            name_text = label(left + .25, .97, name, role="legend")
            version_text = label(left + .25, 1.23, version, role="version", color=self.secondary)
            assert max(text_right(t) for t in (name_text, version_text)) < left + legend_width - .15
        rule(1.52)

        label(self.margin, 1.78, "Project", role="heading")
        columns = (("wall_seconds", 2.30, 6.10, "Wall time", "s"),
                   ("peak_rss_gib", 6.60, 10.40, "Peak memory", "GiB"))
        scales = {}
        for metric, left, right, title, unit in columns:
            heading = label(left, 1.78, title, role="heading")
            reference = "vs " + (self.meta["compilers"][self.baseline]["version"] if self.three_compilers else "nightly")
            note = label(text_right(heading) + .16, 1.78, reference, color=self.secondary)
            assert text_right(note) < right
            # Keep a common zero-based scale across projects and both modes. Expand it
            # for future measurements when needed, reserving room for every value and annotation.
            scale = 0
            for other_mode in self.modes:
                for project in self.projects:
                    baseline = self.means[other_mode, project, self.baseline][metric]
                    for compiler in self.compilers:
                        value = self.means[other_mode, project, compiler][metric]
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
            name = label(self.margin, y, self.labels.get(project, project), role="project")
            assert text_right(name) < columns[0][1] - .15
            if index:
                rule(y - self.row_pitch / 2)
            for metric, left, right, _, unit in columns:
                baseline = self.means[mode, project, self.baseline][metric]
                for compiler, offset in zip(self.compilers, offsets):
                    value = self.means[mode, project, compiler][metric]
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
        text = label(text_right(heading) + .16, self.footer_y, self.shared_arguments[mode], role="footer", color=self.secondary)
        assert text_right(text) < self.figure_width - self.margin, self.shared_arguments[mode]

        # Validate that the full version strings, commands and data labels fit inside the artwork.
        fig.canvas.draw()
        bounds = fig.bbox
        for text in fig.texts:
            box = text.get_window_extent(fig.canvas.get_renderer())
            assert box.x0 >= 0 and box.y0 >= 0 and box.x1 <= bounds.width and box.y1 <= bounds.height, text.get_text()
        fig.savefig(self.here / f"{mode}.png", dpi=200, facecolor="white")
        fig.savefig(self.here / f"{mode}.svg", facecolor="white", metadata={"Date": self.meta["completed_utc"]})
        plt.close(fig)
        path = self.here / f"{mode}.svg"
        svg = path.read_text()
        start = svg.index(">", svg.index("<svg")) + 1
        svg = svg[:start] + "\n<title>" + html.escape(self.titles[mode] + ": tsrs vs TypeScript") + "</title>\n<desc>" + html.escape(self.alt_text(mode)) + "</desc>" + svg[start:]
        path.write_text(svg)
        ET.fromstring(svg)

    def render(self):
        for mode in self.modes:
            self.draw(mode)
        mean_rows = [{"mode": mode, "project": project, "compiler": compiler, "measured_runs": self.runs,
                      **self.means[mode, project, compiler]} for mode in self.modes for project in self.projects for compiler in self.compilers]
        (self.here / "means.json").write_text(json.dumps(mean_rows, indent=2) + "\n")
        with (self.here / "means.csv").open("w", newline="") as f:
            writer = csv.DictWriter(f, fieldnames=list(mean_rows[0]))
            writer.writeheader()
            writer.writerows(mean_rows)
        (self.here / "alt-text.txt").write_text("\n\n".join(self.alt_text(mode) for mode in self.modes) + "\n")

        scope = []
        for (mode, project), preflight in self.preflights.items():
            counts = preflight["compilers"]
            details = []
            for compiler in self.compilers:
                check = counts[compiler]
                description = f"{self.names[compiler]} {self.meta['compilers'][compiler]['version']}: {check['files']:,} files, {check['errors']:,} diagnostics"
                if compiler != "tsrs":
                    match = "match" if check['error_keys'] == counts['tsrs']['error_keys'] else "differ"
                    description += f"; diagnostic locations/codes {match} relative to tsrs"
                details.append(description)
            scope.append(f"{self.titles[mode]} / {self.labels.get(project, project)}: " + ". ".join(details) + ".")
        facts = [
            ("Aggregation", f"Arithmetic mean of {self.runs} measured runs per compiler, project and mode. Warmups excluded. No aggregation across projects."),
            ("Mean peak memory", "Average the individual process peaks, then divide bytes by 2^30 for GiB."),
            ("Comparison labels", f"Reference: {self.reference}, with no comparison labels on its bars. Faster ratio = reference mean wall time / compiler mean wall time; if a compiler is slower, show the reciprocal as × slower. Memory ratio = compiler mean peak RSS / reference mean peak RSS. Calculated before rounding within each project and mode."),
            ("Colors", "; ".join(f"{self.names[c]} {self.meta['compilers'][c]['version']}: {self.colors[c]}" for c in self.compilers)),
            ("Machine", f"{self.cpu}; {self.cores} logical CPUs; {self.ram:g} GiB RAM; macOS {self.meta['machine']['macos']}"),
            ("Compilers", "; ".join(f"{self.meta['compilers'][c]['requested']}: {self.meta['compilers'][c]['version']}" for c in self.compilers) + "; native published binaries."),
            (self.titles["aligned"], f"{self.meta['modes']['aligned']['checkers']} checkers each; default memory policies."),
            ("Maximum resources", f"{self.meta['modes']['maximum']['checkers']} checkers each; tsrs --maxMemory {self.soft_memory}G; Go GOMEMLIMIT={self.soft_memory}GiB. Soft budgets, not hard RSS caps."),
            ("CPU and GC", "; ".join(f"{mode}: " + ", ".join(f"{k}={v if v is not None else 'unset'}" for k, v in next(c for c in self.meta["cells"] if c["mode"] == mode)["environment"].items()) for mode in self.modes)),
            ("Method", f"{self.meta['hyperfine']['version']}; {self.warmups} warmups and {self.runs} measurements; --shell=none; --noEmit --incremental false --pretty false. No timed --extendedDiagnostics."),
            ("Scope", " ".join(scope)),
            ("Source run", self.meta["started_utc"]),
            ("Command footer", "Shared arguments are shown once, read from saved commands for all compilers. <project> represents each row's project path. Compiler-specific flags and environment variables are retained below; GOGC is omitted when unset."),
        ]
        for mode in self.modes:
            facts.append((f"{self.titles[mode]} commands", "; ".join(
                f"{self.names[c]} {self.meta['compilers'][c]['version']}: "
                + " ".join((*self.commands[mode][c]["environment"], self.commands[mode][c]["command"])) for c in self.compilers)))
        if self.display_versions["tsrs"] != self.meta["compilers"]["tsrs"]["version"]:
            facts.append(("tsrs legend version", f"Display override: {self.display_versions['tsrs']}. "
                          f"Measurements remain from {self.meta['compilers']['tsrs']['version']}; the benchmark was not rerun."))
        table = ''.join(f'<tr><th>{html.escape(k)}</th><td>{html.escape(v)}</td></tr>' for k, v in facts)
        sections = []
        for mode in self.modes:
            svg = (self.here / f"{mode}.svg").read_text()
            svg = svg[svg.index("<svg"):]
            sections.append(f'<section id="{mode}"><div class="downloads"><strong>{self.titles[mode]}</strong>'
                            f'<a href="{mode}.png" download>PNG · {self.png_dimensions[0]} × {self.png_dimensions[1]}</a>'
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
        (self.here / "index.html").write_text(page)
        (self.here / "chart-manifest.json").write_text(json.dumps({
            "source": self.meta["started_utc"], "raw_sha256": hashlib.sha256((self.source / "raw.json").read_bytes()).hexdigest(),
            "metadata_sha256": hashlib.sha256((self.source / "metadata.json").read_bytes()).hexdigest(),
            "samples": len(self.raw), "bars_per_chart": len(self.projects) * len(self.compilers) * 2,
            "aggregation": f"arithmetic mean of {self.runs} repetitions per compiler/project/mode",
            "cross_project_aggregation": False, "colors": {c: self.colors[c] for c in self.compilers},
            "compiler_order": self.compilers, "comparison_baseline": self.baseline, "matplotlib": matplotlib.__version__,
            "requirements_sha256": hashlib.sha256((ASSETS / "requirements.txt").read_bytes()).hexdigest(),
            "fonts_sha256": {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                             for p in sorted((ASSETS / "fonts").glob("*.ttf"))},
            "svg_hashsalt": "tsrs-benchmark-v1", "svg_date": self.meta["completed_utc"],
            "png_dimensions": self.png_dimensions, "font": "Inter", "svg_text": "outlined for consistent rendering",
            "text_sizes_pt": self.text_sizes, "text_weights": self.text_weights,
            "text_colors": {"primary": self.ink, "secondary": self.secondary},
            "layout_inches": {"margin": self.margin, "row_pitch": self.row_pitch, "bar_pitch": self.bar_pitch},
            "titles": self.titles, "commands": self.commands, "shared_arguments": self.shared_arguments,
            "display_versions": self.display_versions,
            "measured_versions": {c: self.meta["compilers"][c]["version"] for c in self.compilers},
            "mean_values": mean_rows,
        }, indent=2) + "\n")
        print(f"Created two blog figures in {self.here}")
        print(f"All {len(self.raw)} source measurements validated against Hyperfine; {len(self.projects) * len(self.compilers) * 2} bars in each figure.")


def render(source, destination, display_version=None):
    os.environ.setdefault("MPLCONFIGDIR", str(Path(destination).resolve().parent / ".matplotlib"))
    global matplotlib, plt, FontProperties, Rectangle, Affine2D
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from matplotlib.font_manager import FontProperties
    from matplotlib.patches import Rectangle
    from matplotlib.transforms import Affine2D
    with matplotlib.rc_context():
        matplotlib.rcdefaults()
        BlogCharts(source, destination, display_version).render()
