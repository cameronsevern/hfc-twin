"""Applies the decision rule in docs/tasks/15-held-out-maps-selection.md to what `datagen maps-hybrid` wrote. Reads only Rust's files. All synthetic.

    .venv/bin/python ml/evaluate_maps.py <data dir> <artifacts dir>

Calibrators come from the fit runs, on maps neither the test nor the training saw. Intervals are a 95 % bootstrap over nodes, so runs from one node move together.
"""
import json
import sys
from pathlib import Path

import numpy as np

from evaluate_hybrid import UNPLACEABLE, by_class, ece, ece_spread, fit_calibration, load, names_of, right, score, top_right, wilson

FAMILIES = ("UrbanGrid", "Suburban", "Rural")
WORLDS = [(0, "low"), (0, "high"), (1, "low"), (1, "high"), (9, "low"), (9, "high")]
ESTIMATE_NAMES = {0: "Tap return loss", 1: "Active return loss", 2: "Modem return loss", 3: "Tap port isolation", 4: "Forward noise figure", 5: "Return noise figure", 6: "CIN at reference", 7: "AGC range", 8: "Modem implementation MER", 9: "Hardline and drop loss", 10: "Cable temperature coefficient", 11: "Loop resistance", 12: "Active power draw"}
# Chosen after the result, from what hurt the GNN alone most, so they're a read-out and not in the rule.
LATER_WORLDS = [(8, "low"), (8, "high"), (6, "low")]
DRAWS = 5000


def cluster(r):
    return (r.get("map") or f'{r["family"]}/{r["design_seed"]}', r["profile"], r["node"])


def with_proposal_tops(rows):
    for r in rows:
        r["proposal_tops"] = [[c] for c in r["proposal_top"]]
    return rows


def hits(rows, key):
    """Per run: whether it's placeable, top 1 right and top 3 right, and whether it's healthy and raised an alarm."""
    place, one, three, healthy, alarm = [], [], [], [], []
    for r in rows:
        names = names_of(r[key])
        is_place = r["class"] is not None and r["class"] != UNPLACEABLE
        truth = r["component"] or ""
        place.append(is_place)
        one.append(is_place and bool(names) and names[0] == truth)
        three.append(is_place and truth in names[:3])
        healthy.append(r["class"] is None)
        alarm.append(r["class"] is None and bool(names) and names[0] != "")
    return tuple(np.asarray(v, dtype=float) for v in (place, one, three, healthy, alarm))


def paired(a_rows, b_rows, a_key, b_key):
    """b minus a on identical faults, with a bootstrap over nodes."""
    a_by = {r["seed"]: r for r in a_rows}
    if {r["seed"] for r in b_rows} != set(a_by):
        raise SystemExit("these runs aren't on the same faults")
    a_rows = [a_by[r["seed"]] for r in b_rows]
    pa, a1, a3, _, _ = hits(a_rows, a_key)
    _, b1, b3, _, _ = hits(b_rows, b_key)
    keys = sorted({cluster(r) for r in b_rows})
    index = {k: i for i, k in enumerate(keys)}
    groups = np.asarray([index[cluster(r)] for r in b_rows])
    k = len(keys)

    def by_group(v):
        return np.bincount(groups, weights=v, minlength=k)

    place, d1, d3 = by_group(pa), by_group(b1 - a1), by_group(b3 - a3)
    rng = np.random.default_rng(0)
    draws = rng.multinomial(k, np.full(k, 1 / k), size=DRAWS)
    denominator = np.maximum(draws @ place, 1)
    top1 = draws @ d1 / denominator
    top3 = draws @ d3 / denominator
    n = max(pa.sum(), 1)
    return {
        "nodes": k,
        "placeable": int(pa.sum()),
        "top1_difference": float(d1.sum() / n),
        "top1_95": [float(np.percentile(top1, 2.5)), float(np.percentile(top1, 97.5))],
        "top3_difference": float(d3.sum() / n),
        "top3_95": [float(np.percentile(top3, 2.5)), float(np.percentile(top3, 97.5))],
    }


def full(rows, calibrator, floor=False):
    conf = calibrator.predict_proba(np.asarray([r["calibration_x"] for r in rows]))[:, 1]
    out = {
        "hybrid": score(rows, "hybrid_top", conf.tolist()),
        "hybrid_raw_posterior": score(rows, "hybrid_top", [r["hybrid_posterior"][0] if r["hybrid_posterior"] else 0.0 for r in rows]),
        "ece_spread": ece_spread(conf, top_right(rows)),
        "proposal_only": score(rows, "proposal_tops", [r["proposal_probability"][0] if r["proposal_probability"] else 0.0 for r in rows]),
        "sims": float(np.mean([r["hybrid_sims"] for r in rows])),
        "by_class": by_class(rows, "hybrid_top"),
    }
    if floor:
        out["floor"] = score(rows, "floor_top", [r["floor_posterior"][0] if r["floor_posterior"] else 0.0 for r in rows])
        out["floor_sims"] = float(np.mean([r["floor_sims"] for r in rows]))
        out["floor_by_class"] = by_class(rows, "floor_top")
    return out


def by_field(rows, field):
    out = {}
    for r in rows:
        out.setdefault(r[field], []).append(r)
    return dict(sorted(out.items()))


def side_by_side(gbdt, gnn, field):
    gnn_by = by_field(gnn, field)
    out = {}
    for name, rows in by_field(gbdt, field).items():
        a, b = score(rows, "hybrid_top", [0.0] * len(rows)), score(gnn_by[name], "hybrid_top", [0.0] * len(gnn_by[name]))
        out[name] = {"runs": len(rows), "placeable": a["placeable"], "gbdt_top1": a["top1"], "gbdt_top3": a["top3"], "gbdt_false_alarm": a["healthy_false_alarm"], "gnn_top1": b["top1"], "gnn_top3": b["top3"], "gnn_false_alarm": b["healthy_false_alarm"]}
    return out


def main():
    data, artifacts = Path(sys.argv[1]), Path(sys.argv[2])
    read = lambda name: with_proposal_tops(load(data / name))
    fit = {m: read(f"fit_{m}.jsonl") for m in ("gbdt", "gnn")}
    test = {m: read(f"test_{m}.jsonl") for m in ("gbdt", "gnn")}
    calibrators = {m: fit_calibration(fit[m]) for m in fit}
    results = {"origin": "Synthetic", "note": "Real streets, invented plant, simulated faults and telemetry."}
    results["test"] = {"gbdt": full(test["gbdt"], calibrators["gbdt"], floor=True), "gnn": full(test["gnn"], calibrators["gnn"])}
    results["test_paired"] = {
        "gnn_minus_gbdt": paired(test["gbdt"], test["gnn"], "hybrid_top", "hybrid_top"),
        "gnn_minus_gbdt_proposal_only": paired(test["gbdt"], test["gnn"], "proposal_tops", "proposal_tops"),
        "gbdt_minus_floor": paired(test["gbdt"], test["gbdt"], "floor_top", "hybrid_top"),
    }
    results["by_map"] = side_by_side(test["gbdt"], test["gnn"], "map")
    results["by_profile"] = side_by_side(test["gbdt"], test["gnn"], "profile")

    shipped = read("test_shipped_proposal.jsonl")
    results["shipped_gbdt_proposal_only"] = score(shipped, "proposal_tops", [r["proposal_probability"][0] for r in shipped])
    results["more_data_gbdt_proposal_vs_shipped"] = paired(shipped, test["gbdt"], "proposal_tops", "proposal_tops")

    generated = {}
    for m in ("gbdt", "gnn"):
        rows = [r for f in FAMILIES for r in read(f"generated_{m}_{f}.jsonl")]
        generated[m] = rows
    results["generated_reference"] = {"gbdt": full(generated["gbdt"], calibrators["gbdt"], floor=True), "gnn": full(generated["gnn"], calibrators["gnn"]), "gnn_minus_gbdt": paired(generated["gbdt"], generated["gnn"], "hybrid_top", "hybrid_top")}

    def world(prefix, estimate, end):
        a, b = read(f"{prefix}_gbdt_{estimate}_{end}.jsonl"), read(f"{prefix}_gnn_{estimate}_{end}.jsonl")
        sa, sb = score(a, "hybrid_top", [0.0] * len(a)), score(b, "hybrid_top", [0.0] * len(b))
        return {"gbdt_top3": sa["top3"], "gnn_top3": sb["top3"], "gbdt_top1": sa["top1"], "gnn_top1": sb["top1"], "gbdt_false_alarm": sa["healthy_false_alarm"], "gnn_false_alarm": sb["healthy_false_alarm"], "placeable": sa["placeable"], "gnn_minus_gbdt": paired(a, b, "hybrid_top", "hybrid_top")}

    worlds = {f"{ESTIMATE_NAMES[e]} {end}": world("world", e, end) for e, end in WORLDS}
    results["worlds"] = worlds
    later = {f"{ESTIMATE_NAMES[e]} {end}": world("worldr", e, end) for e, end in LATER_WORLDS}
    results["worlds_added_after_the_result"] = later
    proposals = {}
    for estimate, name in ESTIMATE_NAMES.items():
        for end in ("low", "high"):
            a, b = read(f"worldp_gbdt_{estimate}_{end}.jsonl"), read(f"worldp_gnn_{estimate}_{end}.jsonl")
            sa, sb = score(a, "proposal_tops", [0.0] * len(a)), score(b, "proposal_tops", [0.0] * len(b))
            proposals[f"{name} {end}"] = {"gbdt_top3": sa["top3"], "gnn_top3": sb["top3"], "gbdt_false_alarm": sa["healthy_false_alarm"], "gnn_false_alarm": sb["healthy_false_alarm"]}
    results["worlds_proposal_only"] = proposals

    latency = json.loads((artifacts / "gnn_latency.json").read_text())
    results["latency"] = latency
    p = results["test_paired"]["gnn_minus_gbdt"]
    a, b = results["test"]["gbdt"]["hybrid"], results["test"]["gnn"]["hybrid"]
    worst = min(worlds.values(), key=lambda w: w["gnn_top3"])
    rule = {
        "localization": p["top3_95"][0] > 0 or (p["top1_95"][0] > 0 and p["top3_95"][1] >= 0),
        "false_alarms": b["healthy_false_alarm"] <= a["healthy_false_alarm"] + 0.05,
        "calibration": b["ece"] <= a["ece"] + 0.02,
        "when_the_sim_is_wrong": worst["gnn_top3"] >= worst["gbdt_top3"] - 0.03,
        "latency": latency["gnn_ms"] < 50.0,
    }
    results["decision"] = {"rule": rule, "gnn_replaces_gbdt": all(rule.values())}
    (artifacts / "results_maps.json").write_text(json.dumps(results, indent=1))
    print(json.dumps({k: results[k] for k in ("test_paired", "decision", "by_profile", "more_data_gbdt_proposal_vs_shipped")}, indent=1))
    for m in ("gbdt", "gnn"):
        print(m, "test", json.dumps(results["test"][m]["hybrid"]), "proposal", json.dumps(results["test"][m]["proposal_only"]))
    print("floor", json.dumps(results["test"]["gbdt"]["floor"]))
    for name, w in {**worlds, **later}.items():
        print(name, json.dumps({k: v for k, v in w.items() if k != "gnn_minus_gbdt"}))

    for m in ("gbdt", "gnn"):
        path = artifacts / f"{m}_maps.json"
        model = json.loads(path.read_text())
        model["calibration"] = {"coef": calibrators[m].coef_[0].tolist(), "intercept": float(calibrators[m].intercept_[0])}
        path.write_text(json.dumps(model))


if __name__ == "__main__":
    main()
