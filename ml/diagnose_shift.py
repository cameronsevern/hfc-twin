"""Why the GNN collapses when the sim is wrong, and what fixes it: new data, new inputs, or both. Reads Rust's files only (invariant 6). All synthetic.

    .venv/bin/python ml/diagnose_shift.py <saved|gbdt|<variant>> <out dir>

Scored the model alone, on the calibrate maps in moved worlds, never the test maps or the reserves. `run_diagnose_shift.sh` runs it all.
"""
import json
import pickle
import sys
import time
from pathlib import Path

import numpy as np

import select_maps as sm

DATA = Path("ml/data")
DIAG = DATA / "diag"
NOMINAL = [DATA / "rows_generated.jsonl", DATA / "rows_maps_train.jsonl"]
WORLDS = {"nominal": DATA / "rows_maps_calibrate.jsonl", **{w: DIAG / f"cal_{w}.jsonl" for w in ("mer_low", "mer_high", "tap_low", "tap_high", "active_high", "cin_low", "varied", "mer_minus_10", "mer_minus_20", "taps_7db", "cable_plus_30", "severe")}}
# A settled reading minus what the sim expects, so an estimate that's off moves it on every modem at once. The rest are counts, states and places.
DRIFTS = [0, 1, 2, 3, 4, 5, 6, 8, *range(13, 21)]
SERVICE_GROUP_INPUTS = 11  # train_gnn.GLOBAL before any variant adds to it.
# data: which moved rows join the nominal ones. inputs: whether each modem reads relative to the node's median, and whether the median is kept.
VARIANTS = {
    "relative": {"data": None, "inputs": "relative"},
    "relative_medians": {"data": None, "inputs": "medians"},
    "varied": {"data": "train_varied", "inputs": "raw"},
    "varied_blind": {"data": "train_varied_blind", "inputs": "raw"},
    "varied_relative": {"data": "train_varied", "inputs": "medians"},
}


def relative(t, keep_medians):
    """Each modem's drift against its own node's median: an error shared by every modem cancels, and a fault on part of the node stands out."""
    import torch

    x, m = t["x"].clone(), t["modems"]
    medians = torch.zeros(len(DRIFTS), dtype=x.dtype)
    for k, j in enumerate(DRIFTS):
        present = m[x[m, sm_width() + j] > 0]
        if len(present):
            medians[k] = x[present, j].median()
            x[present, j] -= medians[k]
    # train_gnn.GLOBAL has already grown to hold the medians, so tensorize read that many extra columns: keep only the service group's own.
    g = torch.cat([t["g"][:SERVICE_GROUP_INPUTS], medians]) if keep_medians else t["g"]
    return {**t, "x": x, "g": g}


def sm_width():
    from train_gnn import WIDTH

    return WIDTH


def tensorizer(inputs):
    from train_gnn import tensorize

    if inputs == "raw":
        return tensorize
    return lambda r: (lambda t: relative(t, inputs == "medians"))(tensorize(r))


def score_worlds(net, tensorize):
    val = sm.gnn_load(WORLDS["nominal"], "calibrate", tensorize)
    out = {}
    for name, path in WORLDS.items():
        target = val if name == "nominal" else sm.gnn_load(path, "calibrate", tensorize)
        out[name] = sm.gnn_score(net, val, target)
        print(name, {k: round(out[name][k], 3) for k in ("top1", "top3", "healthy_false_alarm", "ece")}, flush=True)
    return out


def load_saved():
    import torch

    from train_gnn import Net

    model = json.loads(Path("ml/artifacts/gnn_maps.json").read_text())
    net = Net(model["hidden"])

    def put(layer, saved):
        layer.weight.data = torch.tensor(saved["w"], dtype=torch.float32)
        layer.bias.data = torch.tensor(saved["b"], dtype=torch.float32)

    put(net.enc[0], model["enc"][0])
    put(net.enc[2], model["enc"][1])
    for layer, saved in zip(net.round, model["rounds"]):
        put(layer[0], saved)
    put(net.mix[0], model["mix"][0])
    put(net.mix[2], model["mix"][1])
    net.kind.weight.data = torch.tensor(model["kind_embedding"], dtype=torch.float32)
    put(net.score[0], model["score"][0])
    put(net.score[2], model["score"][1])
    return net


def main():
    which, out = sys.argv[1], Path(sys.argv[2])
    t0 = time.time()
    results = {"origin": "Synthetic", "variant": which}
    if which == "saved":
        # The chosen GNN as it is, then the same weights reading relative inputs with no retraining at all.
        net = load_saved()
        results["as_trained"] = score_worlds(net, tensorizer("raw"))
        results["relative_at_inference_only"] = score_worlds(net, tensorizer("relative"))
    elif which == "gbdt":
        settings = {"max_iter": 400, "max_leaf_nodes": 31}
        val = sm.gbdt_load(WORLDS["nominal"], "calibrate")
        worlds = {name: (val if name == "nominal" else sm.gbdt_load(path, "calibrate")) for name, path in WORLDS.items()}
        nominal = [i for path in NOMINAL for i in sm.gbdt_load(path, "train")]
        for name, data in (("nominal", None), ("varied", "train_varied"), ("varied_blind", "train_varied_blind")):
            items = nominal + (sm.gbdt_load(DIAG / f"{data}.jsonl", "train") if data else [])
            comp = sm.gbdt_fit(items, settings)
            results[name] = {w: sm.gbdt_score(comp, val, target) for w, target in worlds.items()}
            with open(out / f"gbdt_{name}.pkl", "wb") as f:
                pickle.dump(comp, f)
            print(name, {w: (round(s["top1"], 3), round(s["top3"], 3), round(s["healthy_false_alarm"], 3)) for w, s in results[name].items()}, flush=True)
    else:
        import torch

        import train_gnn

        variant = VARIANTS[which]
        torch.set_num_threads(3)
        if variant["inputs"] == "medians":
            train_gnn.GLOBAL += len(DRIFTS)
        tensorize = tensorizer(variant["inputs"])
        items = [i for path in NOMINAL for i in sm.gnn_load(path, "train", tensorize)]
        if variant["data"]:
            items += sm.gnn_load(DIAG / f'{variant["data"]}.jsonl', "train", tensorize)
        print(which, len(items), "rows", flush=True)
        settings = {"hidden": 64, "epochs": 16}
        net = sm.gnn_fit(items, settings, which)
        torch.save(net.state_dict(), out / f"{which}.pt")
        results["rows"] = len(items)
        results["worlds"] = score_worlds(net, tensorize)
        if variant["inputs"] == "raw":
            model = sm.export_gnn(net, settings, results["worlds"]["nominal"]["temperature"], {k: items[0][k] for k in ("observation_schema", "fault_schema")}, f"gnn-{which}")
            (out / f"gnn_{which}.json").write_text(json.dumps(model))
    results["seconds"] = time.time() - t0
    (out / f"{which}.json").write_text(json.dumps(results, indent=1))


if __name__ == "__main__":
    main()
