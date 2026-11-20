"""The read-outs beside the rule in docs/tasks/17-spectrum-model.md: where the frozen model fails, and its worst captures drawn. All synthetic.

    ml/data/venv-gpu/bin/python ml/spectrum_readouts.py <data dir> <artifacts dir> <figure.svg>

Run after `evaluate_spectrum.py test`. It changes no setting, and nothing it prints goes back into the model.
"""
import json
import sys
from pathlib import Path

import numpy as np
import torch

from evaluate_spectrum import VISIBLE_DB, Truth, marks, predict
from train_spectrum import BIN_MHZ, SHAPES, cache

BANDS = [(3, 6), (6, 10), (10, 20), (20, 1000)]
PANEL_W, PANEL_H, GAP = 460, 150, 34


def recall_by_band(found, truth):
    out = {}
    for s, name in enumerate(SHAPES):
        row = {}
        for lo, hi in BANDS:
            pick = (truth.worst[:, s] >= lo) & (truth.worst[:, s] < hi)
            row[f"{lo}-{hi}"] = {"captures": int(pick.sum()), "named": float(found[pick, s].mean()) if pick.any() else None}
        out[name] = row
    return out


def fall_error(falls, truth, mean):
    hit = (truth.fall >= VISIBLE_DB) & truth.present
    err = np.abs(falls - truth.fall)
    out = {"all": float(err[hit].mean()), "by_shape": {}, "by_fall": {}}
    for s, name in enumerate(SHAPES):
        bins = ((truth.bits >> s) & 1).astype(bool) & hit
        out["by_shape"][name] = {"bins": int(bins.sum()), "mae_db": float(err[bins].mean()), "signed_db": float((falls - truth.fall)[bins].mean()), "share_of_error": float(err[bins].sum() / err[hit].sum())}
    low = ((truth.bits >> SHAPES.index("LowMer")) & 1).astype(bool)
    out["without_lowmer_bins"] = float(err[hit & ~low].mean())
    # A bin read at the floor can't show how far below it the sim's value went.
    floor = hit & (np.asarray(mean) == 0)
    out["bins_read_at_0_db"] = {"bins": int(floor.sum()), "mae_db": float(err[floor].mean()) if floor.any() else None}
    for lo, hi in BANDS:
        bins = hit & (truth.fall >= lo) & (truth.fall < hi)
        out["by_fall"][f"{lo}-{hi}"] = {"bins": int(bins.sum()), "mae_db": float(err[bins].mean()), "signed_db": float((falls - truth.fall)[bins].mean())}
    return out


def several(found, truth):
    visible = truth.worst >= VISIBLE_DB
    rows = np.nonzero(visible.sum(axis=1) >= 2)[0]
    named = sum(int(found[i, visible[i]].sum()) for i in rows)
    total = int(visible[rows].sum())
    return {"captures": len(rows), "shapes_visible": total, "share_of_those_named": named / max(total, 1)}


def panel(x, y, title, note, first, mean, present, fall):
    """One capture: the curve as read, and that curve plus its labelled fall."""
    n = int(present.sum())
    at = lambda k, v: (x + 34 + k / max(n - 1, 1) * (PANEL_W - 44), y + PANEL_H - 20 - v / 50.0 * (PANEL_H - 58))
    def path(values):
        return " ".join(f"{a:.1f},{b:.1f}" for a, b in (at(k, v) for k, v in enumerate(values)))
    read = mean[present] / 4.0
    before = read + fall[present]
    parts = [f'<text x="{x}" y="{y + 12}" class="t">{title}</text>', f'<text x="{x}" y="{y + 25}" class="n">{note}</text>']
    for level in (0, 25, 50):
        ax, ay = at(0, level)
        parts.append(f'<line x1="{ax:.1f}" y1="{ay:.1f}" x2="{x + PANEL_W - 10}" y2="{ay:.1f}" class="g"/><text x="{ax - 4:.1f}" y="{ay + 3:.1f}" class="a" text-anchor="end">{level}</text>')
    parts.append(f'<polyline points="{path(np.clip(before, 0, 50))}" class="b"/><polyline points="{path(np.clip(read, 0, 50))}" class="r"/>')
    ax, ay = at(0, 0)
    bx, _ = at(n - 1, 0)
    parts.append(f'<text x="{ax:.1f}" y="{ay + 13:.1f}" class="a">{first:.0f} MHz</text><text x="{bx:.1f}" y="{ay + 13:.1f}" class="a" text-anchor="end">{first + (n - 1) * BIN_MHZ:.0f} MHz</text>')
    return "".join(parts)


def main():
    data_dir, artifacts, figure = Path(sys.argv[1]), Path(sys.argv[2]), Path(sys.argv[3])
    model = json.loads((artifacts / "spectrum_model.json").read_text())
    state = torch.load(data_dir / "fits" / "final.pt", map_location="cpu")
    data = cache(data_dir / "test.jsonl")
    truth = Truth(data)
    probs, falls = predict(state, model["settings"]["width"], data)
    found, _, unknown, normal = marks(probs, falls, truth, model["thresholds"])
    mean = np.asarray(data["mean"], np.float64)
    shape_list = lambda i: ", ".join(SHAPES[s] for s in np.nonzero(found[i])[0]) or ("don't know" if unknown[i] else "normal")
    true_list = lambda i: ", ".join(f"{SHAPES[s]} {truth.worst[i, s]:.0f} dB" for s in np.nonzero(truth.worst[i] >= VISIBLE_DB)[0]) or "nothing"
    worst = truth.fall.max(axis=1)
    healthy_mean = np.array([mean[i][truth.present[i]].mean() / 4.0 for i in np.nonzero(truth.healthy)[0]])
    picks = []
    # The deepest falls called normal, then each shape's deepest capture that wasn't named, then the worst false alarms.
    for i in np.argsort(-np.where(normal, worst, -1))[:2]:
        picks.append(("Deepest fall called normal", i))
    for name in ("LowMer", "Wave", "RollOff", "Dip"):
        s = SHAPES.index(name)
        miss = np.where(~found[:, s] & (truth.worst[:, s] >= VISIBLE_DB), truth.worst[:, s], -1)
        picks.append((f"Deepest {name} not named", int(np.argmax(miss))))
    alarm = np.where(truth.healthy & ~normal, (falls * truth.present).max(axis=1), -1e9)
    picks.append(("Healthy, worst false alarm", int(np.argmax(alarm))))
    visible = (truth.worst >= VISIBLE_DB).sum(axis=1)
    missed_many = np.where(visible >= 2, visible - np.array([found[i, truth.worst[i] >= VISIBLE_DB].sum() for i in range(len(visible))]), -1)
    picks.append(("Several faults, most missed", int(np.argmax(missed_many * 100 + worst / 100))))
    cols = 2
    rows = (len(picks) + 1) // cols
    width, height = cols * (PANEL_W + GAP), rows * (PANEL_H + GAP) + 28
    svg = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" font-family="sans-serif"><style>.t{{font-size:12px;font-weight:600;fill:#222}}.n{{font-size:10px;fill:#555}}.a{{font-size:9px;fill:#777}}.g{{stroke:#ddd;stroke-width:1}}.r{{fill:none;stroke:#b5442e;stroke-width:1.2}}.b{{fill:none;stroke:#3a6ea5;stroke-width:1}}rect{{fill:#fff}}</style><rect width="{width}" height="{height}"/>']
    svg.append('<text x="0" y="14" class="n">Synthetic captures from the test maps. Red: mean RxMER per bin as read, dB. Blue: red plus the labelled fall, cut at 50 dB.</text>')
    listing = []
    for k, (title, i) in enumerate(picks):
        x, y = k % cols * (PANEL_W + GAP), k // cols * (PANEL_H + GAP) + 28
        i = int(i)
        note = f"true: {true_list(i)}. read: {shape_list(i)}."
        svg.append(panel(x, y, title, note, data["meta"][i]["first_mhz"], mean[i], truth.present[i], truth.fall[i]))
        listing.append({"title": title, "true": true_list(i), "read": shape_list(i), "worst_fall_db": float(worst[i]), "map": data["meta"][i]["map"]})
    svg.append("</svg>")
    figure.parent.mkdir(parents=True, exist_ok=True)
    figure.write_text("".join(svg))
    result = {"origin": "Synthetic", "note": "Read after the frozen model was scored. Nothing here feeds back.", "recall_by_own_fall": recall_by_band(found, truth), "healthy_channel_mean_db": {"p5": float(np.percentile(healthy_mean, 5)), "under_35": float((healthy_mean < 35).mean()), "under_30": float((healthy_mean < 30).mean()), "lowest": float(healthy_mean.min())}, "fall_error": fall_error(falls, truth, mean), "several_at_once": several(found, truth), "worst": listing}
    (artifacts / "results_spectrum_readouts.json").write_text(json.dumps(result, indent=1))
    print(json.dumps(result, indent=1))


if __name__ == "__main__":
    main()
