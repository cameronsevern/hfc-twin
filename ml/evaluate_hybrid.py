"""Scores what `datagen hybrid` wrote, and fits the confidence the twin reports.

    .venv/bin/python ml/evaluate_hybrid.py <out.json> <fit dir> <test dir> <model.json>

Each dir holds hybrid_<Family>.jsonl. The fit dir's runs and the test dir's come from different seeds, so different layouts. For a family, the
confidence is fitted on the *other* families' fit runs and scored on this family's test runs: nothing about the rows scored went into either the
proposal model or the calibrator. The shipped calibration is then fitted on every fit run and written into the model.

Reads only Rust's files. All synthetic. The bar is `docs/DESIGN.md`'s: top 3 at least 80 %, beating the floor, ECE under 5 %.
"""
import json
import math
import sys
from pathlib import Path

import numpy as np
from sklearn.linear_model import LogisticRegression

UNPLACEABLE = "UpstreamIngress"
FAMILIES = ("UrbanGrid", "Suburban", "Rural")


def load(path):
    with open(path) as f:
        return [json.loads(line) for line in f]


def ece(conf, correct, bins=10):
    conf, correct = np.asarray(conf), np.asarray(correct, dtype=float)
    total = 0.0
    for b in range(bins):
        lo, hi = b / bins, (b + 1) / bins
        mask = (conf > lo) & (conf <= hi) if b else (conf >= lo) & (conf <= hi)
        if mask.any():
            total += mask.mean() * abs(conf[mask].mean() - correct[mask].mean())
    return float(total)


def wilson(k, n, z=1.96):
    """A 95 % interval, since a few hundred runs don't pin a share down to a point."""
    if n == 0:
        return [0.0, 0.0]
    p = k / n
    d = 1 + z * z / n
    c = (p + z * z / (2 * n)) / d
    h = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d
    return [c - h, c + h]


def names_of(tops):
    """Ranked components with repeats dropped: a component tried as two causes is one place to send a truck."""
    out = []
    for components in tops:
        name = components[0] if components else ""
        if name not in out:
            out.append(name)
    return out


def right(r, name):
    truth = r["component"] or ""
    return name == truth or (r["class"] == UNPLACEABLE and name.endswith("root"))


def score(rows, top_key, confidence):
    top1 = top3 = placeable = false_alarm = healthy = 0
    conf, correct = [], []
    for r, c in zip(rows, confidence):
        names = names_of(r[top_key])
        conf.append(c)
        correct.append(bool(names) and right(r, names[0]))
        if r["class"] is None:
            healthy += 1
            false_alarm += bool(names) and names[0] != ""
        elif r["class"] != UNPLACEABLE:
            placeable += 1
            top1 += bool(names) and names[0] == (r["component"] or "")
            top3 += (r["component"] or "") in names[:3]
    return {
        "rows": len(rows),
        "placeable": placeable,
        "top1": top1 / max(placeable, 1),
        "top3": top3 / max(placeable, 1),
        "top3_95": wilson(top3, placeable),
        "healthy_false_alarm": false_alarm / max(healthy, 1),
        "ece": ece(conf, correct),
        "mean_confidence": float(np.mean(conf)),
    }


def by_class(rows, top_key):
    out = {}
    for r in rows:
        if r["class"] is None or r["class"] == UNPLACEABLE:
            continue
        hit = (r["component"] or "") in names_of(r[top_key])[:3]
        n, k = out.get(r["class"], (0, 0))
        out[r["class"]] = (n + 1, k + hit)
    return {c: {"n": n, "top3": k / n} for c, (n, k) in sorted(out.items())}


def ece_spread(conf, correct, draws=2000):
    """How much of a measured ECE this few runs would show anyway: the ECE of outcomes drawn from these same confidences, so calibrated by
    construction, beside a bootstrap interval on the measured one. Read-outs only. The bar is judged on the measured ECE."""
    rng = np.random.default_rng(0)
    conf, correct = np.asarray(conf), np.asarray(correct, dtype=float)
    if_calibrated = [ece(conf, rng.uniform(size=len(conf)) < conf) for _ in range(draws)]
    boot = []
    for _ in range(draws):
        i = rng.integers(0, len(conf), len(conf))
        boot.append(ece(conf[i], correct[i]))
    return {
        "accuracy_top_answer": float(correct.mean()),
        "mean_confidence": float(conf.mean()),
        "ece_95": [float(np.percentile(boot, 2.5)), float(np.percentile(boot, 97.5))],
        "ece_if_calibrated_mean": float(np.mean(if_calibrated)),
        "ece_if_calibrated_95th": float(np.percentile(if_calibrated, 95)),
    }


def top_right(rows, key="hybrid_top"):
    return [bool(names_of(r[key])) and right(r, names_of(r[key])[0]) for r in rows]


def fit_calibration(rows):
    x = np.asarray([r["calibration_x"] for r in rows])
    y = np.asarray(top_right(rows), dtype=int)
    return LogisticRegression(C=1.0, max_iter=2000).fit(x, y)


def main():
    out, fit_dir, test_dir, model_path = sys.argv[1:]
    fit = {f: load(Path(fit_dir) / f"hybrid_{f}.jsonl") for f in FAMILIES}
    test = {f: load(Path(test_dir) / f"hybrid_{f}.jsonl") for f in FAMILIES}
    results = {"origin": "Synthetic", "families": {}}
    for family in FAMILIES:
        rows = test[family]
        calibrator = fit_calibration([r for f in FAMILIES if f != family for r in fit[f]])
        calibrated = calibrator.predict_proba(np.asarray([r["calibration_x"] for r in rows]))[:, 1]
        entry = {
            "floor": score(rows, "floor_top", [r["floor_posterior"][0] if r["floor_posterior"] else 0.0 for r in rows]),
            "hybrid_raw_posterior": score(rows, "hybrid_top", [r["hybrid_posterior"][0] if r["hybrid_posterior"] else 0.0 for r in rows]),
            "hybrid": score(rows, "hybrid_top", calibrated.tolist()),
            "hybrid_ece_spread": ece_spread(calibrated, top_right(rows)),
            "floor_sims": float(np.mean([r["floor_sims"] for r in rows])),
            "hybrid_sims": float(np.mean([r["hybrid_sims"] for r in rows])),
            "floor_by_class": by_class(rows, "floor_top"),
            "hybrid_by_class": by_class(rows, "hybrid_top"),
        }
        results["families"][family] = entry
        print(family, "sims", entry["floor_sims"], entry["hybrid_sims"])
        for k in ("floor", "hybrid_raw_posterior", "hybrid"):
            print("  ", k, json.dumps({a: (round(b, 3) if isinstance(b, float) else b) for a, b in entry[k].items()}))
        for c in entry["hybrid_by_class"]:
            print(f"   {c:26} n={entry['hybrid_by_class'][c]['n']:3} hybrid {entry['hybrid_by_class'][c]['top3']:.2f}  floor {entry['floor_by_class'][c]['top3']:.2f}")
    shipped = fit_calibration([r for f in FAMILIES for r in fit[f]])
    # Fresh layouts, but of families the shipped calibrator saw, so this is not the bar: it says what the page's number means on known ground.
    all_test = [r for f in FAMILIES for r in test[f]]
    shipped_conf = shipped.predict_proba(np.asarray([r["calibration_x"] for r in all_test]))[:, 1]
    results["shipped_calibrator_on_seen_families"] = {"ece": ece(shipped_conf, top_right(all_test)), **ece_spread(shipped_conf, top_right(all_test))}
    print("shipped calibrator, families it saw", json.dumps(results["shipped_calibrator_on_seen_families"]))
    Path(out).write_text(json.dumps(results, indent=1))

    model = json.loads(Path(model_path).read_text())
    model["calibration"] = {"coef": shipped.coef_[0].tolist(), "intercept": float(shipped.intercept_[0])}
    Path(model_path).write_text(json.dumps(model))
    print("calibration written to", model_path, model["calibration"])


if __name__ == "__main__":
    main()
