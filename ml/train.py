"""Model selection on Rust's files. Reads only what `datagen` wrote and reimplements no physics (CLAUDE.md, invariant 6).

    .venv/bin/python ml/train.py ml/data/rows.jsonl ml/artifacts

Held out by family and by profile, never by random sample. All synthetic.
"""
import json
import math
import sys
import time
from pathlib import Path

import numpy as np
from sklearn.ensemble import HistGradientBoostingClassifier

UNPLACEABLE = "UpstreamIngress"  # §7.3: modem data doesn't place it, so it's scored on class alone.
RPD_ONLY = ("Rpd", "Cin", "Gcp", "Timing", "Rfm")


def load(path):
    with open(path) as f:
        return [json.loads(line) for line in f]


def truth_component(r):
    """Which candidate is right. Healthy is the empty name. Upstream ingress is proposed at the root, as the diagnoser places it."""
    if r["class"] is None:
        return ""
    if r["class"] == UNPLACEABLE:
        return next(c["component"] for c in r["candidates"] if c["kind"] == "Root")
    return r["component"]


def to_x(c):
    return [np.nan if v is None else v for v in c["x"]]


def matrix(rows):
    xs, ys, groups = [], [], []
    for g, r in enumerate(rows):
        truth = truth_component(r)
        for c in r["candidates"]:
            xs.append(to_x(c))
            ys.append(1 if c["component"] == truth else 0)
            groups.append(g)
    return np.asarray(xs, dtype=np.float64), np.asarray(ys), np.asarray(groups)


def softmax_by_group(scores, groups, n, temperature=1.0):
    out = []
    start = 0
    for g in range(n):
        end = start
        while end < len(groups) and groups[end] == g:
            end += 1
        s = scores[start:end] / temperature
        s = s - s.max()
        p = np.exp(s)
        out.append(p / p.sum())
        start = end
    return out


def fit_temperature(scores, groups, ys, n):
    best = (1.0, float("inf"))
    for t in np.geomspace(0.25, 16.0, 49):
        nll, start = 0.0, 0
        for p in softmax_by_group(scores, groups, n, t):
            y = ys[start : start + len(p)]
            nll -= math.log(max(float(p[y.argmax()]), 1e-12))
            start += len(p)
        if nll < best[1]:
            best = (float(t), nll)
    return best[0]


def ece(conf, correct, bins=10):
    conf, correct = np.asarray(conf), np.asarray(correct, dtype=float)
    total = 0.0
    for b in range(bins):
        lo, hi = b / bins, (b + 1) / bins
        mask = (conf > lo) & (conf <= hi) if b else (conf >= lo) & (conf <= hi)
        if mask.any():
            total += mask.mean() * abs(conf[mask].mean() - correct[mask].mean())
    return float(total)


def component_metrics(rows, ranked_names, confidences):
    """Top-k on placeable faults, healthy rows' false alarms, and calibration of the top answer over every row."""
    top1 = top3 = placeable = false_alarm = healthy = 0
    conf, correct = [], []
    for r, names, c in zip(rows, ranked_names, confidences):
        truth = truth_component(r)
        conf.append(c)
        correct.append(bool(names) and names[0] == truth)
        if r["class"] is None:
            healthy += 1
            false_alarm += bool(names) and names[0] != ""
        elif r["class"] != UNPLACEABLE:
            placeable += 1
            top1 += bool(names) and names[0] == truth
            top3 += truth in names[:3]
    return {
        "rows": len(rows),
        "placeable": placeable,
        "top1": top1 / max(placeable, 1),
        "top3": top3 / max(placeable, 1),
        "healthy_false_alarm": false_alarm / max(healthy, 1),
        "ece": ece(conf, correct),
    }


def floor_metrics(rows):
    names, conf = [], []
    for r in rows:
        f = r["floor"]
        names.append([(components[0] if components else "") for components in f["components"]])
        conf.append(f["posterior"][0] if f["posterior"] else 0.0)
    return component_metrics(rows, names, conf)


def train_gbdt(rows):
    x, y, _ = matrix(rows)
    # Rare positives: weight them to balance, and let the softmax across candidates sort out the rest.
    weight = np.where(y == 1, (y == 0).sum() / max((y == 1).sum(), 1), 1.0)
    comp = HistGradientBoostingClassifier(max_iter=200, learning_rate=0.08, max_leaf_nodes=31, random_state=1)
    comp.fit(x, y, sample_weight=weight)
    pos_x, pos_y = [], []
    for r in rows:
        if r["class"] is None:
            continue
        truth = truth_component(r)
        for c in r["candidates"]:
            if c["component"] == truth:
                pos_x.append(to_x(c))
                pos_y.append(r["class"])
    cls = HistGradientBoostingClassifier(max_iter=120, learning_rate=0.08, max_leaf_nodes=15, random_state=1)
    cls.fit(np.asarray(pos_x, dtype=np.float64), np.asarray(pos_y))
    return comp, cls


def gbdt_scores(model, rows):
    x, y, groups = matrix(rows)
    return model.decision_function(x), y, groups


def rank_gbdt(rows, scores, groups, temperature):
    names, conf = [], []
    for r, p in zip(rows, softmax_by_group(scores, groups, len(rows), temperature)):
        order = np.argsort(-p, kind="stable")
        names.append([r["candidates"][i]["component"] for i in order])
        conf.append(float(p[order[0]]))
    return names, conf


def export_trees(model):
    trees = []
    for iteration in model._predictors:
        per_class = []
        for predictor in iteration:
            nodes = []
            for n in predictor.nodes:
                leaf = bool(n["is_leaf"])
                threshold = float(n["num_threshold"])
                nodes.append(
                    {
                        "f": int(n["feature_idx"]),
                        "t": threshold if not math.isinf(threshold) else 1e308,
                        "l": -1 if leaf else int(n["left"]),
                        "r": -1 if leaf else int(n["right"]),
                        "v": float(n["value"]),
                        "m": bool(n["missing_go_to_left"]),
                    }
                )
            per_class.append(nodes)
        trees.append(per_class)
    return {"baseline": np.asarray(model._baseline_prediction).reshape(-1).tolist(), "trees": trees}


def export_model(comp, cls, temperature, rows, feature_names, name):
    return {
        "name": name,
        "observation_schema": rows[0]["observation_schema"],
        "fault_schema": rows[0]["fault_schema"],
        "trained_on": "Synthetic",
        "features": feature_names,
        "component": export_trees(comp),
        "class": export_trees(cls),
        "classes": [str(c) for c in cls.classes_],
        "temperature": temperature,
        # Task 24: rows on version 2 plant say which plant they're on, and the model records it (invariant 3).
        **({"plant_schema": rows[0]["versions"]["plant"]} if "versions" in rows[0] else {}),
    }


def split_val(rows):
    """A fifth of the training designs, whole, for the temperature: never a random row, or a design leaks into both."""
    keys = sorted({(r["family"], r["profile"], r["design_seed"]) for r in rows})
    val_keys = set(keys[::5])
    key = lambda r: (r["family"], r["profile"], r["design_seed"])
    return [r for r in rows if key(r) not in val_keys], [r for r in rows if key(r) in val_keys]


FOLDS = [
    ("family:UrbanGrid", lambda r: r["family"] == "UrbanGrid"),
    ("family:Suburban", lambda r: r["family"] == "Suburban"),
    ("family:Rural", lambda r: r["family"] == "Rural"),
    ("profile:lowsplit (legacy)", lambda r: "lowsplit" in r["profile"]),
    ("profile:highsplit (DAA)", lambda r: "highsplit" in r["profile"]),
]


def fold_rows(rows, name, is_test):
    test = [r for r in rows if is_test(r)]
    pool = [r for r in rows if not is_test(r)]
    # A model trained on low-split has never seen an RPD fault, and those classes can't be held out from themselves.
    no_rpd = lambda r: r["class"] is None or not r["class"].startswith(RPD_ONLY)
    if "lowsplit" in name:
        pool = [r for r in pool if no_rpd(r)]
    if "highsplit" in name:
        test = [r for r in test if no_rpd(r)]
    return test, pool


def main():
    data, out = sys.argv[1], Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    rows = load(data)
    feature_names = json.loads((out / "feature_names.json").read_text())
    print(len(rows), "rows;", sum(1 for r in rows if r["floor"]), "with the floor's answer", flush=True)
    results = {"origin": "Synthetic", "rows": len(rows), "folds": {}}
    for name, is_test in FOLDS:
        test, pool = fold_rows(rows, name, is_test)
        train, val = split_val(pool)
        t0 = time.time()
        comp, cls = train_gbdt(train)
        vs, vy, vg = gbdt_scores(comp, val)
        temperature = fit_temperature(vs, vg, vy, len(val))
        fit_s = time.time() - t0
        ts, _, tg = gbdt_scores(comp, test)
        metrics = component_metrics(test, *rank_gbdt(test, ts, tg, temperature))
        px, py = [], []
        for r in test:
            if r["class"] is None:
                continue
            truth = truth_component(r)
            for c in r["candidates"]:
                if c["component"] == truth:
                    px.append(to_x(c))
                    py.append(r["class"])
        class_acc = float((cls.predict(np.asarray(px, dtype=np.float64)) == np.asarray(py)).mean()) if px else 0.0
        floor_rows = [r for r in test if r["floor"]]
        same = None
        if floor_rows:
            fs, _, fg = gbdt_scores(comp, floor_rows)
            same = component_metrics(floor_rows, *rank_gbdt(floor_rows, fs, fg, temperature))
        results["folds"][name] = {
            "train_rows": len(train),
            "gbdt": {**metrics, "class_given_place": class_acc, "temperature": temperature, "fit_s": fit_s},
            "gbdt_on_floor_rows": same,
            "floor": floor_metrics(floor_rows) if floor_rows else None,
            "floor_sims_per_run": float(np.mean([r["floor"]["simulations"] for r in floor_rows])) if floor_rows else None,
        }
        print(name, json.dumps(results["folds"][name]["gbdt"]), "\n   same rows:", same, "\n   floor:", results["folds"][name]["floor"], flush=True)
        if name.startswith("family:"):
            short = name.split(":")[1]
            (out / f"gbdt_holdout_{short}.json").write_text(json.dumps(export_model(comp, cls, temperature, rows, feature_names, f"gbdt-holdout-{short}")))

    train, val = split_val(rows)
    comp, cls = train_gbdt(train)
    vs, vy, vg = gbdt_scores(comp, val)
    temperature = fit_temperature(vs, vg, vy, len(val))
    (out / "proposal_gbdt.json").write_text(json.dumps(export_model(comp, cls, temperature, rows, feature_names, "gbdt")))
    # What Rust's evaluator has to reproduce.
    parity = []
    for r in val[:6]:
        x = np.asarray([to_x(c) for c in r["candidates"]], dtype=np.float64)
        parity.append({"x": [c["x"] for c in r["candidates"]], "component": comp.decision_function(x).tolist(), "class": cls.decision_function(x).tolist()})
    (out / "parity.json").write_text(json.dumps(parity))
    (out / "results_gbdt.json").write_text(json.dumps(results, indent=1))


if __name__ == "__main__":
    main()
