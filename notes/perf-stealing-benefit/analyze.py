#!/usr/bin/env python3
"""Summarize stealbench.py results and draw the report's charts (light and dark SVGs).

usage: analyze.py results.jsonl outdir
Writes outdir/summary.json, outdir/tables.md and the charts.
"""
import json, math, os, random, sys, collections
from pathlib import Path
import numpy as np
import matplotlib
matplotlib.use("Agg")
matplotlib.rcParams["axes.unicode_minus"] = False
matplotlib.rcParams["svg.fonttype"] = "none"
import matplotlib.pyplot as plt

THEMES = {
    "light": {"surface": "#fcfcfb", "text": "#0b0b0b", "text2": "#52514e", "grid": "#e4e3df",
              "series": ["#2a78d6", "#eb6834", "#1baf7a"], "zero": "#8a8985"},
    "dark": {"surface": "#1a1a19", "text": "#f0efec", "text2": "#c3c2b7", "grid": "#33322f",
             "series": ["#3987e5", "#d95926", "#199e70"], "zero": "#77766f"},
}
ARM_LABEL = {"steal": "stealing (default)", "locality": "no stealing", "go": "Go assignment + history"}
B = 4000  # bootstrap resamples


def load(path):
    cells = collections.defaultdict(list)
    meta = {}
    for line in open(path):
        r = json.loads(line)
        key = (r["project"], r["checkers"], r["arm"])
        cells[key].append(r["wall_s"])
        meta.setdefault((r["project"], r["checkers"]), {}).setdefault(r["arm"], set()).add((r["exit"], r["errors"], r["stdout_sha"]))
    return cells, meta


def boot_ratio(a, b, rng):
    """Median(a)/median(b) and a 95% percentile bootstrap interval."""
    a, b = np.asarray(a), np.asarray(b)
    est = np.median(a) / np.median(b)
    ia = rng.integers(0, len(a), (B, len(a)))
    ib = rng.integers(0, len(b), (B, len(b)))
    rs = np.median(a[ia], axis=1) / np.median(b[ib], axis=1)
    return est, np.percentile(rs, 2.5), np.percentile(rs, 97.5)


def summarize(cells, meta):
    rng = np.random.default_rng(7)
    projects = sorted({k[0] for k in cells})
    counts = sorted({k[1] for k in cells})
    arms = [a for a in ("steal", "locality", "go") if any(k[2] == a for k in cells)]
    out = {"projects": {}, "counts": counts, "arms": arms}
    for p in projects:
        pr = {}
        for n in counts:
            c = {}
            for arm in arms:
                v = cells.get((p, n, arm), [])
                if v:
                    c[arm] = {"median": float(np.median(v)), "min": float(min(v)), "n": len(v),
                              "iqr": [float(np.percentile(v, 25)), float(np.percentile(v, 75))]}
            # Stealing's change: steal / other - 1 (negative = stealing faster).
            for other in arms:
                if other != "steal" and (p, n, other) in cells and (p, n, "steal") in cells:
                    est, lo, hi = boot_ratio(cells[(p, n, "steal")], cells[(p, n, other)], rng)
                    c[f"steal_vs_{other}"] = {"ratio": float(est), "lo": float(lo), "hi": float(hi)}
            same = meta[(p, n)]
            c["outputs_identical"] = len(set().union(*same.values())) == 1
            pr[n] = c
        out["projects"][p] = pr
    # Aggregates per count: geometric mean of per-project ratios (bootstrap over projects and runs), and summed medians.
    agg = {}
    for n in counts:
        a = {}
        for other in arms:
            if other == "steal":
                continue
            ps = [p for p in projects if (p, n, other) in cells and (p, n, "steal") in cells]
            logs = [math.log(out["projects"][p][n][f"steal_vs_{other}"]["ratio"]) for p in ps]
            bs = []
            for _ in range(2000):
                pick = rng.integers(0, len(ps), len(ps))
                vals = []
                for i in pick:
                    p = ps[i]
                    s = np.asarray(cells[(p, n, "steal")]); o = np.asarray(cells[(p, n, other)])
                    vals.append(math.log(np.median(rng.choice(s, len(s))) / np.median(rng.choice(o, len(o)))))
                bs.append(math.exp(np.mean(vals)))
            sum_s = sum(out["projects"][p][n]["steal"]["median"] for p in ps)
            sum_o = sum(out["projects"][p][n][other]["median"] for p in ps)
            a[other] = {"geomean_ratio": math.exp(np.mean(logs)), "lo": float(np.percentile(bs, 2.5)),
                        "hi": float(np.percentile(bs, 97.5)), "sum_steal_s": sum_s, "sum_other_s": sum_o,
                        "sum_ratio": sum_s / sum_o, "projects": len(ps),
                        "faster": sum(1 for p in ps if out["projects"][p][n][f"steal_vs_{other}"]["hi"] < 1),
                        "slower": sum(1 for p in ps if out["projects"][p][n][f"steal_vs_{other}"]["lo"] > 1)}
        agg[n] = a
    out["aggregate"] = agg
    return out


def style(ax, t):
    ax.set_facecolor(t["surface"])
    for s in ("top", "right", "left"):
        ax.spines[s].set_visible(False)
    ax.spines["bottom"].set_color(t["grid"])
    ax.tick_params(colors=t["text2"], labelsize=9, length=0)
    ax.grid(axis="x", color=t["grid"], linewidth=0.8)
    ax.set_axisbelow(True)


def pct(r):
    return (r - 1) * 100


def chart_per_project(s, theme, path):
    t = THEMES[theme]
    counts = s["counts"]
    ref = max(counts)
    projects = sorted(s["projects"], key=lambda p: s["projects"][p][ref]["locality"]["median"])
    fig, axes = plt.subplots(1, len(counts), figsize=(2.6 * len(counts) + 1.6, 0.32 * len(projects) + 1.4),
                             sharey=True, facecolor=t["surface"])
    allv = [pct(s["projects"][p][n]["steal_vs_locality"][k]) for p in projects for n in counts for k in ("lo", "hi")]
    lim = max(abs(min(allv)), abs(max(allv))) * 1.08
    for ax, n in zip(axes, counts):
        style(ax, t)
        ax.axvline(0, color=t["zero"], linewidth=1)
        for y, p in enumerate(projects):
            c = s["projects"][p][n]["steal_vs_locality"]
            ax.plot([pct(c["lo"]), pct(c["hi"])], [y, y], color=t["series"][0], linewidth=2, solid_capstyle="round")
            ax.plot(pct(c["ratio"]), y, "o", markersize=6, color=t["series"][0],
                    markeredgecolor=t["surface"], markeredgewidth=1.5)
        ax.set_xlim(-lim, lim)
        ax.set_title(f"{n} checkers", color=t["text"], fontsize=10, loc="left")
        ax.set_xlabel("wall time change, %", color=t["text2"], fontsize=9)
    axes[0].set_yticks(range(len(projects)))
    axes[0].set_yticklabels(projects, color=t["text"], fontsize=9)
    axes[0].invert_yaxis()
    fig.suptitle("Stealing vs no stealing: wall-time change per project\n(median ratio with 95% CI; left of 0 = stealing faster)",
                 color=t["text"], fontsize=11, x=0.01, ha="left")
    fig.tight_layout(rect=(0, 0, 1, 0.93))
    fig.savefig(path, facecolor=t["surface"])
    plt.close(fig)


def chart_aggregate(s, theme, path):
    t = THEMES[theme]
    counts = s["counts"]
    fig, ax = plt.subplots(figsize=(7.2, 3.6), facecolor=t["surface"])
    style(ax, t)
    ax.grid(axis="y", color=t["grid"], linewidth=0.8)
    ax.grid(axis="x", visible=False)
    ax.axhline(0, color=t["zero"], linewidth=1)
    ax.text(counts[-1] + 0.15, 0, "stealing (baseline)", color=t["text2"], fontsize=9, va="center")
    for i, other in enumerate([a for a in s["arms"] if a != "steal"], start=1):
        ys = [pct(1 / s["aggregate"][n][other]["geomean_ratio"]) for n in counts]
        lo = [pct(1 / s["aggregate"][n][other]["hi"]) for n in counts]
        hi = [pct(1 / s["aggregate"][n][other]["lo"]) for n in counts]
        col = t["series"][i]
        ax.fill_between(counts, lo, hi, color=col, alpha=0.15, linewidth=0)
        ax.plot(counts, ys, color=col, linewidth=2, marker="o", markersize=7, markeredgecolor=t["surface"],
                markeredgewidth=1.5)
        ax.text(counts[-1] + 0.15, ys[-1], ARM_LABEL[other], color=t["text"], fontsize=9, va="center")
    ax.set_xticks(counts)
    ax.set_xlabel("checker count (--checkers)", color=t["text2"], fontsize=9)
    ax.set_ylabel("slower than stealing, % (geomean)", color=t["text2"], fontsize=9)
    ax.set_xlim(counts[0] - 0.3, counts[-1] + 3.2)
    ax.set_title("Cost of turning stealing off, geometric mean over projects (95% CI band)", color=t["text"],
                 fontsize=11, loc="left")
    fig.tight_layout()
    fig.savefig(path, facecolor=t["surface"])
    plt.close(fig)


def chart_large(s, theme, path, top=6):
    t = THEMES[theme]
    counts = s["counts"]
    ref = min(counts)
    projects = sorted(s["projects"], key=lambda p: -s["projects"][p][ref]["steal"]["median"])[:top]
    cols = 3
    rows = math.ceil(len(projects) / cols)
    fig, axes = plt.subplots(rows, cols, figsize=(10, 3.0 * rows), facecolor=t["surface"])
    axes = np.atleast_1d(axes).ravel()
    for ax, p in zip(axes, projects):
        style(ax, t)
        ax.grid(axis="y", color=t["grid"], linewidth=0.8)
        ax.grid(axis="x", visible=False)
        for i, arm in enumerate(s["arms"]):
            ys = [s["projects"][p][n][arm]["median"] for n in counts]
            ax.plot(counts, ys, color=t["series"][i], linewidth=2, marker="o", markersize=6,
                    markeredgecolor=t["surface"], markeredgewidth=1.2, label=ARM_LABEL[arm])
        ax.set_title(p, color=t["text"], fontsize=10, loc="left")
        ax.set_xticks(counts)
        ax.set_ylim(bottom=0)
        ax.set_xlabel("checker count (--checkers)", color=t["text2"], fontsize=8)
        ax.set_ylabel("wall s (median)", color=t["text2"], fontsize=8)
    for ax in axes[len(projects):]:
        ax.axis("off")
    h, l = axes[0].get_legend_handles_labels()
    leg = fig.legend(h, l, loc="upper right", ncol=3, frameon=False, fontsize=9)
    for txt in leg.get_texts():
        txt.set_color(t["text"])
    fig.suptitle("Wall time by checker count, largest projects", color=t["text"], fontsize=11, x=0.01, ha="left")
    fig.tight_layout(rect=(0, 0, 1, 0.92))
    fig.savefig(path, facecolor=t["surface"])
    plt.close(fig)


def tables(s):
    counts = s["counts"]
    lines = ["| project | " + " | ".join(f"{n}: steal / no stealing / go (s)" for n in counts) + " |",
             "| --- | " + " | ".join("---:" for _ in counts) + " |"]
    for p in sorted(s["projects"], key=lambda p: -s["projects"][p][max(counts)]["locality"]["median"]):
        cells = []
        for n in counts:
            c = s["projects"][p][n]
            cells.append(" / ".join(f"{c[a]['median']:.2f}" for a in s["arms"]))
        lines.append(f"| {p} | " + " | ".join(cells) + " |")
    lines += ["", "| project | " + " | ".join(f"{n} checkers" for n in counts) + " |",
              "| --- | " + " | ".join("---:" for _ in counts) + " |"]
    for p in sorted(s["projects"], key=lambda p: -s["projects"][p][max(counts)]["locality"]["median"]):
        cells = []
        for n in counts:
            c = s["projects"][p][n]["steal_vs_locality"]
            cells.append(f"{pct(c['ratio']):+.1f}% [{pct(c['lo']):+.1f}, {pct(c['hi']):+.1f}]")
        lines.append(f"| {p} | " + " | ".join(cells) + " |")
    return "\n".join(lines) + "\n"


def main():
    cells, meta = load(sys.argv[1])
    out = Path(sys.argv[2]); out.mkdir(parents=True, exist_ok=True)
    s = summarize(cells, meta)
    (out / "summary.json").write_text(json.dumps(s, indent=1, default=str))
    (out / "tables.md").write_text(tables(s))
    for theme in THEMES:
        sfx = "" if theme == "light" else "-dark"
        chart_per_project(s, theme, out / f"per-project{sfx}.svg")
        chart_aggregate(s, theme, out / f"aggregate{sfx}.svg")
        chart_large(s, theme, out / f"largest{sfx}.svg")
        if os.environ.get("PREVIEW"):
            chart_per_project(s, theme, out / f"per-project{sfx}.png")
            chart_aggregate(s, theme, out / f"aggregate{sfx}.png")
            chart_large(s, theme, out / f"largest{sfx}.png")
    print(json.dumps(s["aggregate"], indent=1))


if __name__ == "__main__":
    main()
