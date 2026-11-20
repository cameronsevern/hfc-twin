"""Applies task 15's decision rule, docs/DESIGN.md's bar and task 24's new bars to what `datagen ab-hybrid` and `ab-fiber` wrote on
version 2 plant (docs/tasks/24-as-built-diagnosis-and-selection.md). Reads only Rust's files (invariant 6). All synthetic: real
streets on some maps, invented plant, simulated faults and telemetry.

    .venv/bin/python ml/evaluate_as_built.py <data dir> <artifacts dir>

The rules were fixed before any result, in tasks 15 and 24, and are applied as written here. Intervals are a 95 % bootstrap over nodes,
as task 15's, or Wilson's for a share.
"""
import json
import sys
from pathlib import Path

import numpy as np

from evaluate_hybrid import UNPLACEABLE, fit_calibration, load, names_of, score, wilson
from evaluate_maps import FAMILIES, WORLDS, ESTIMATE_NAMES, full, paired, side_by_side, with_proposal_tops

FIBER_CLASSES = ("FiberCut", "FiberEventLoss", "OpticalTransmitterDrift")


def read(data, name):
    path = data / name
    return with_proposal_tops(load(path)) if path.exists() else None


def home_bars(rows, key):
    """Task 24's first new bar. Inside: the top answer names a part of the faulted home. Outside: the top answer is inside some home."""
    inside_n = inside_k = outside_n = outside_k = 0
    for r in rows:
        names = names_of(r[key])
        top = names[0] if names else ""
        if r["class"] == "InHomeWiring":
            inside_n += 1
            inside_k += top in set(r["in_home"])
        elif r["class"] is not None:
            outside_n += 1
            outside_k += top in set(r["home_places"])
    return {
        "inside": {"n": inside_n, "in_its_home": inside_k / max(inside_n, 1), "95": wilson(inside_k, inside_n), "bar": 0.80},
        "outside": {"n": outside_n, "called_in_a_home": outside_k / max(outside_n, 1), "95": wilson(outside_k, outside_n), "bar": 0.10},
    }


def fiber_bars(rows):
    """Task 24's other two: a cut on a cable carrying two or more nodes named within one element, and a coax fault not called fiber."""
    cuts = [r for r in rows if r["case"] == "cut_shared" and r["carries"] >= 2]
    named = sum(1 for r in cuts if r["verdict"] == "Fiber" and (r["place"] == r["truth"] or (r["hops_off"] is not None and r["hops_off"] <= 1)))
    coax = [r for r in rows if r["case"] == "coax"]
    blamed = sum(1 for r in coax if r["verdict"] == "Fiber")
    return {
        "regions": len({(r["family"], r["design_seed"], r["profile"]) for r in rows}),
        "nodes_per_region": sorted({r["nodes"] for r in rows}),
        "names_the_cable": {"n": len(cuts), "share": named / max(len(cuts), 1), "95": wilson(named, len(cuts)), "bar": 0.90},
        "does_not_blame_fiber": {"n": len(coax), "called_fiber": blamed / max(len(coax), 1), "95": wilson(blamed, len(coax)), "bar": 0.05},
    }


def coax_called_fiber(rows, key, classes_key):
    """On the node's own diagnosis: a coax fault whose top answer is a fiber class."""
    n = k = 0
    for r in rows:
        if r["class"] is None or r["class"] in FIBER_CLASSES:
            continue
        n += 1
        top = r[classes_key][0] if r[classes_key] else None
        k += top in FIBER_CLASSES
    return {"n": n, "called_fiber": k / max(n, 1), "95": wilson(k, n)}


def stress(rows, field):
    out = {}
    for r in rows:
        out.setdefault(str(r.get(field)), []).append(r)
    return {name: {k: v for k, v in score(group, "hybrid_top", [0.0] * len(group)).items() if k in ("rows", "placeable", "top1", "top3", "top3_95", "healthy_false_alarm")} for name, group in sorted(out.items())}


def main():
    data, artifacts = Path(sys.argv[1]), Path(sys.argv[2])
    fit = {m: read(data, f"fit_{m}.jsonl") for m in ("gbdt", "gnn")}
    test = {m: read(data, f"test_{m}.jsonl") for m in ("gbdt", "gnn")}
    calibrators = {m: fit_calibration(fit[m]) for m in fit}
    results = {"origin": "Synthetic", "note": "Real streets on some maps, invented version 2 plant, simulated faults and telemetry. Nothing here is a field result."}
    results["test"] = {"gbdt": full(test["gbdt"], calibrators["gbdt"], floor=True), "gnn": full(test["gnn"], calibrators["gnn"])}
    results["test_paired"] = {
        "gnn_minus_gbdt": paired(test["gbdt"], test["gnn"], "hybrid_top", "hybrid_top"),
        "gnn_minus_gbdt_proposal_only": paired(test["gbdt"], test["gnn"], "proposal_tops", "proposal_tops"),
        "gbdt_minus_floor": paired(test["gbdt"], test["gbdt"], "floor_top", "hybrid_top"),
    }
    results["by_map"] = side_by_side(test["gbdt"], test["gnn"], "map")
    results["by_profile"] = side_by_side(test["gbdt"], test["gnn"], "profile")

    shipped = read(data, "test_shipped_proposal.jsonl")
    if shipped is not None:
        results["shipped_gbdt_proposal_only"] = score(shipped, "proposal_tops", [r["proposal_probability"][0] if r["proposal_probability"] else 0.0 for r in shipped])

    def world(estimate, end):
        a, b = read(data, f"world_gbdt_{estimate}_{end}.jsonl"), read(data, f"world_gnn_{estimate}_{end}.jsonl")
        sa, sb = score(a, "hybrid_top", [0.0] * len(a)), score(b, "hybrid_top", [0.0] * len(b))
        return {"gbdt_top3": sa["top3"], "gnn_top3": sb["top3"], "gbdt_top1": sa["top1"], "gnn_top1": sb["top1"], "gbdt_false_alarm": sa["healthy_false_alarm"], "gnn_false_alarm": sb["healthy_false_alarm"], "placeable": sa["placeable"], "gnn_minus_gbdt": paired(a, b, "hybrid_top", "hybrid_top")}

    worlds = {f"{ESTIMATE_NAMES[e]} {end}": world(e, end) for e, end in WORLDS}
    results["worlds"] = worlds
    proposals = {}
    for estimate, name in ESTIMATE_NAMES.items():
        for end in ("low", "high"):
            a, b = read(data, f"worldp_gbdt_{estimate}_{end}.jsonl"), read(data, f"worldp_gnn_{estimate}_{end}.jsonl")
            if a is None or b is None:
                continue
            sa, sb = score(a, "proposal_tops", [0.0] * len(a)), score(b, "proposal_tops", [0.0] * len(b))
            proposals[f"{name} {end}"] = {"gbdt_top3": sa["top3"], "gnn_top3": sb["top3"], "gbdt_false_alarm": sa["healthy_false_alarm"], "gnn_false_alarm": sb["healthy_false_alarm"]}
    results["worlds_proposal_only"] = proposals

    # Task 15's five-part rule, as written.
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

    # docs/DESIGN.md's bar, beside both.
    floor = results["test"]["gbdt"]["floor"]
    beat = results["test_paired"]["gbdt_minus_floor"]
    results["design_bar"] = {
        m: {
            "top3": results["test"][m]["hybrid"]["top3"],
            "top3_95": results["test"][m]["hybrid"]["top3_95"],
            "top3_at_least_80": results["test"][m]["hybrid"]["top3"] >= 0.80,
            "ece": results["test"][m]["hybrid"]["ece"],
            "ece_under_5": results["test"][m]["hybrid"]["ece"] < 0.05,
            "floor_top3": floor["top3"],
        }
        for m in ("gbdt", "gnn")
    }
    results["design_bar"]["gbdt"]["beats_the_floor"] = beat["top3_95"][0] > 0
    results["design_bar"]["gnn"]["beats_the_floor"] = b["top3"] > floor["top3"]

    # Task 24's new bars.
    results["new_bars"] = {
        "inside_or_outside_the_home": {m: home_bars(test[m], "hybrid_top") for m in ("gbdt", "gnn")},
        "inside_or_outside_the_home_floor": home_bars(test["gbdt"], "floor_top"),
        "coax_called_fiber_by_the_node": {m: coax_called_fiber(test[m], "hybrid_top", "hybrid_classes") for m in ("gbdt", "gnn")},
    }
    fiber = data / "fiber.jsonl"
    if fiber.exists():
        results["new_bars"].update(fiber_bars(load(fiber)))

    # Stress tests, reported and part of no rule.
    results["stress"] = {
        m: {"installation": stress(test[m], "installation"), "design_mode": stress(test[m], "design_mode"), "over_ceiling": stress(test[m], "over_ceiling")}
        for m in ("gbdt", "gnn")
    }
    for m in ("gbdt", "gnn"):
        folds = artifacts / f"results_maps_{m}{'_folds' if m == 'gnn' else ''}.json"
        if folds.exists():
            results["stress"][m]["held_out_folds"] = json.loads(folds.read_text()).get("folds")
    generated = {m: [r for f in FAMILIES for r in (read(data, f"generated_{m}_{f}.jsonl") or [])] for m in ("gbdt", "gnn")}
    if all(generated.values()):
        results["generated_reference"] = {"gbdt": full(generated["gbdt"], calibrators["gbdt"], floor=True), "gnn": full(generated["gnn"], calibrators["gnn"])}

    # One read-out on the fully loaded profiles, part of no rule.
    readout = {m: read(data, f"full_{m}.jsonl") for m in ("gbdt", "gnn")}
    results["full_profiles_readout"] = {m: full(rows, calibrators[m]) for m, rows in readout.items() if rows}

    (artifacts / "results_as_built.json").write_text(json.dumps(results, indent=1))
    print(json.dumps({k: results[k] for k in ("test_paired", "decision", "design_bar", "new_bars")}, indent=1))
    for m in ("gbdt", "gnn"):
        print(m, "test", json.dumps(results["test"][m]["hybrid"]), "proposal", json.dumps(results["test"][m]["proposal_only"]))
    print("floor", json.dumps(floor))
    for name, w in worlds.items():
        print(name, json.dumps({k: v for k, v in w.items() if k != "gnn_minus_gbdt"}))
    for m in ("gbdt", "gnn"):
        path = artifacts / f"{m}_maps.json"
        model = json.loads(path.read_text())
        model["calibration"] = {"coef": calibrators[m].coef_[0].tolist(), "intercept": float(calibrators[m].intercept_[0])}
        path.write_text(json.dumps(model))


if __name__ == "__main__":
    main()
