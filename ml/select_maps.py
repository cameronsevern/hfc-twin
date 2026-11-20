"""Model selection data and fits for held-out maps (docs/tasks/15-held-out-maps-selection.md). Reads Rust's files only (invariant 6). All synthetic.

    .venv/bin/python ml/select_maps.py gbdt <out dir> <generated.jsonl> <train maps.jsonl> <calibrate maps.jsonl>
    .venv/bin/python ml/select_maps.py gnn  <out dir> <generated.jsonl> <train maps.jsonl> <calibrate maps.jsonl>

Each writes a learning curve, a settings sweep and stress folds, all scored on the calibrate maps or on what a fold held out, and the final model.
They never read the test maps. GBDT and GNN are separate processes, since OpenMP and fork don't mix.
"""
import json
import multiprocessing as mp
import os
import sys
import time
from pathlib import Path

# Task 24's queue trains the GBDT beside sensitivity's two threads and the spectrum CNN, and OpenMP spin-waits badly when it
# has more threads than free cores, so the GBDT takes twelve unless told otherwise. Set before sklearn loads.
if len(sys.argv) > 1 and sys.argv[1] == "gbdt":
    os.environ.setdefault("OMP_NUM_THREADS", "12")

import numpy as np

from train import (
    RPD_ONLY,
    component_metrics,
    export_model,
    fit_temperature,
    gbdt_scores,
    rank_gbdt,
    softmax_by_group,
    to_x,
    truth_component,
)

FRACTIONS = [0.125, 0.25, 0.5, 1.0]
GBDT_DEFAULT = {"max_iter": 200, "max_leaf_nodes": 31}
GBDT_SWEEP = [{"max_iter": 200, "max_leaf_nodes": 31}, {"max_iter": 400, "max_leaf_nodes": 31}, {"max_iter": 200, "max_leaf_nodes": 63}, {"max_iter": 400, "max_leaf_nodes": 63}]
GNN_DEFAULT = {"hidden": 64, "epochs": 8}
GNN_SWEEP = [{"hidden": 64, "epochs": 8}, {"hidden": 64, "epochs": 16}, {"hidden": 128, "epochs": 8}, {"hidden": 128, "epochs": 16}]
FAMILIES = ("UrbanGrid", "Suburban", "Rural")


def rows_of(path):
    with open(path) as f:
        for line in f:
            yield json.loads(line)


def layout(r):
    """The same streets on either profile are one layout, so a subsample takes them whole."""
    return r.get("map") or f'{r["family"]}/{r["design_seed"]}'


def subsample(items, fraction):
    """Nested, so a larger fraction holds every layout of a smaller one."""
    keys = sorted({i["layout"] for i in items})
    order = np.random.default_rng(1).permutation(len(keys))
    kept = {keys[i] for i in order[: max(1, round(len(keys) * fraction))]}
    return [i for i in items if i["layout"] in kept]


def nll_of(probabilities, ys):
    return float(np.mean([-np.log(max(float(p[y]), 1e-12)) for p, y in zip(probabilities, ys)]))


def meta(r, source):
    return {
        "source": source,
        "layout": layout(r),
        "family": r["family"],
        "profile": r["profile"],
        "class": r["class"],
        "component": r["component"],
        "observation_schema": r["observation_schema"],
        "fault_schema": r["fault_schema"],
        **({"versions": r["versions"]} if "versions" in r else {}),
    }


def is_held_out(fold, item):
    kind, name = fold.split(":")
    return item["family"] == name if kind == "family" else name in item["profile"]


def fold_items(items, fold):
    """Generated layouts only, the same shape as step 12's folds. A model trained on low-split has never seen an RPD fault."""
    generated = [i for i in items if i["source"] == "generated"]
    test = [i for i in generated if is_held_out(fold, i)]
    train = [i for i in generated if not is_held_out(fold, i)]
    no_rpd = lambda i: i["class"] is None or not i["class"].startswith(RPD_ONLY)
    if fold == "profile:lowsplit":
        train = [i for i in train if no_rpd(i)]
    if fold == "profile:highsplit":
        test = [i for i in test if no_rpd(i)]
    return train, test


FOLDS = [f"family:{f}" for f in FAMILIES] + ["profile:lowsplit", "profile:highsplit"]


# ---------- GBDT ----------


def gbdt_load(path, source):
    out = []
    for r in rows_of(path):
        if truth_component(r) is None or not any(c["component"] == truth_component(r) for c in r["candidates"]):
            continue
        out.append({**meta(r, source), "candidates": [{"component": c["component"], "kind": c["kind"]} for c in r["candidates"]], "x": np.asarray([to_x(c) for c in r["candidates"]], dtype=np.float64)})
    return out


def gbdt_matrix(items):
    x = np.concatenate([i["x"] for i in items])
    t = [truth_component(i) for i in items]
    y = np.concatenate([np.asarray([1 if c["component"] == tr else 0 for c in i["candidates"]]) for i, tr in zip(items, t)])
    groups = np.concatenate([np.full(len(i["candidates"]), g) for g, i in enumerate(items)])
    return x, y, groups


def fillable(x):
    """A column with no value at all in what a fit sees can't be split on, and sklearn's binning refuses it, so it's a constant
    there: the trees are the same, and what the model's given later is untouched."""
    empty = np.isnan(x).all(axis=0)
    if empty.any():
        x = x.copy()
        x[:, empty] = 0.0
    return x


def gbdt_fit(items, settings):
    from sklearn.ensemble import HistGradientBoostingClassifier

    x, y, _ = gbdt_matrix(items)
    x = fillable(x)
    weight = np.where(y == 1, (y == 0).sum() / max((y == 1).sum(), 1), 1.0)
    comp = HistGradientBoostingClassifier(learning_rate=0.08, random_state=1, **settings)
    comp.fit(x, y, sample_weight=weight)
    return comp


def gbdt_score(comp, val, test=None):
    vx, vy, vg = gbdt_matrix(val)
    vs = comp.decision_function(vx)
    t = fit_temperature(vs, vg, vy, len(val))
    p = softmax_by_group(vs, vg, len(val), t)
    ys = [next(k for k, c in enumerate(i["candidates"]) if c["component"] == truth_component(i)) for i in val]
    out = {"temperature": t, "nll": nll_of(p, ys)}
    target = val if test is None else test
    tx, _, tg = gbdt_matrix(target)
    out.update(component_metrics(target, *rank_gbdt(target, comp.decision_function(tx), tg, t)))
    return out


def gbdt_main(out, generated, train_maps, calibrate):
    t0 = time.time()
    items = gbdt_load(generated, "generated") + gbdt_load(train_maps, "maps")
    val = gbdt_load(calibrate, "calibrate")
    print(len(items), "training rows,", len(val), "calibrate rows,", f"{time.time() - t0:.0f}s", flush=True)
    # A model names the features it reads (task 24): a column with no value anywhere in training, such as a reading the
    # shipped profiles don't report, isn't one it can read, so it's left out of the model and of what it's given.
    names = json.loads((out / "feature_names.json").read_text())
    keep = np.zeros(len(names), dtype=bool)
    for i in items:
        keep |= ~np.isnan(i["x"]).all(axis=0)
    if not keep.all():
        print("left out, empty in training:", [n for n, k in zip(names, keep) if not k], flush=True)
    for i in items + val:
        i["x"] = i["x"][:, keep]
    names = [n for n, k in zip(names, keep) if k]
    results = {"origin": "Synthetic", "train_rows": len(items), "calibrate_rows": len(val), "curve": [], "sweep": [], "folds": {}}
    for fraction in FRACTIONS:
        part = subsample(items, fraction)
        s = time.time()
        comp = gbdt_fit(part, GBDT_DEFAULT)
        entry = {"fraction": fraction, "rows": len(part), "layouts": len({i["layout"] for i in part}), **gbdt_score(comp, val), "fit_s": time.time() - s}
        results["curve"].append(entry)
        print("curve", entry, flush=True)
    best = None
    for settings in GBDT_SWEEP:
        s = time.time()
        comp = gbdt_fit(items, settings)
        entry = {**settings, **gbdt_score(comp, val), "fit_s": time.time() - s}
        results["sweep"].append(entry)
        print("sweep", entry, flush=True)
        if best is None or entry["nll"] < best[0]["nll"]:
            best = (entry, comp, settings)
    entry, comp, settings = best
    results["chosen"] = settings
    for fold in FOLDS:
        train, test = fold_items(items, fold)
        cut = sorted({i["layout"] for i in train})[::5]
        fit = [i for i in train if i["layout"] not in cut]
        hold = [i for i in train if i["layout"] in cut]
        results["folds"][fold] = {"train_rows": len(fit), **gbdt_score(gbdt_fit(fit, settings), hold, test)}
        print(fold, results["folds"][fold], flush=True)
    from sklearn.ensemble import HistGradientBoostingClassifier

    pos_x, pos_y = [], []
    for i in items:
        if i["class"] is None:
            continue
        k = next(k for k, c in enumerate(i["candidates"]) if c["component"] == truth_component(i))
        pos_x.append(i["x"][k])
        pos_y.append(i["class"])
    cls = HistGradientBoostingClassifier(max_iter=120, learning_rate=0.08, max_leaf_nodes=15, random_state=1).fit(fillable(np.asarray(pos_x)), np.asarray(pos_y))
    first = {"observation_schema": items[0]["observation_schema"], "fault_schema": items[0]["fault_schema"], **({"versions": items[0]["versions"]} if "versions" in items[0] else {})}
    (out / "gbdt_maps.json").write_text(json.dumps(export_model(comp, cls, entry["temperature"], [first], names, "gbdt-maps")))
    (out / "results_maps_gbdt.json").write_text(json.dumps(results, indent=1))
    # What Rust has to reproduce on a few calibrate rows, as train.py's parity file (task 24).
    parity = []
    for i in val[:6]:
        scores = cls.decision_function(i["x"])
        scores = scores.reshape(len(i["x"]), -1)
        parity.append({"x": [[None if np.isnan(v) else float(v) for v in row] for row in i["x"]], "component": comp.decision_function(i["x"]).tolist(), "class": scores.tolist()})
    (out / "parity.json").write_text(json.dumps(parity))


# ---------- GNN ----------

SHARED = {}


def gnn_load(path, source, tensorize):
    out = []
    for r in rows_of(path):
        t = tensorize(r)
        if t["y"] is None:
            continue
        out.append({**meta(r, source), "tensor": t, "candidates": [{"component": c["component"], "kind": c["kind"]} for c in r["candidates"]]})
    return out


def gnn_fit(items, settings, label):
    import torch
    from torch import nn

    from train_gnn import Net

    torch.manual_seed(1)
    net = Net(settings["hidden"])
    opt = torch.optim.Adam(net.parameters(), lr=2e-3)
    rng = np.random.default_rng(1)
    loss_fn = nn.CrossEntropyLoss()
    # The rate halves over the same share of training at any length.
    halve_every = max(settings["epochs"] // 3, 1)
    t0 = time.time()
    for epoch in range(settings["epochs"]):
        order = rng.permutation(len(items))
        opt.param_groups[0]["lr"] = 2e-3 * (0.5 ** (epoch // halve_every))
        for start in range(0, len(order), 16):
            opt.zero_grad()
            for i in order[start : start + 16]:
                t = items[i]["tensor"]
                (loss_fn(net(t).unsqueeze(0), torch.tensor([t["y"]])) / 16).backward()
            opt.step()
        print(label, "epoch", epoch, f"{time.time() - t0:.0f}s", flush=True)
    return net


def gnn_score(net, val, test=None):
    from train_gnn import fit_t, ranked, scores_of

    vs = scores_of(net, [i["tensor"] for i in val])
    ys = [i["tensor"]["y"] for i in val]
    t = fit_t(vs, ys)
    ps = []
    for s in vs:
        p = np.exp((s - s.max()) / t)
        ps.append(p / p.sum())
    out = {"temperature": t, "nll": nll_of(ps, ys)}
    target = val if test is None else test
    ts = scores_of(net, [i["tensor"] for i in target]) if test is not None else vs
    out.update(component_metrics(target, *ranked(target, ts, t)))
    return out


def gnn_task(task):
    import torch

    torch.set_num_threads(2)
    kind, arg, settings = task
    s = time.time()
    items, val = SHARED["items"], SHARED["val"]
    if kind == "curve":
        part = subsample(items, arg)
        net = gnn_fit(part, settings, f"curve {arg}")
        entry = {"fraction": arg, "rows": len(part), "layouts": len({i["layout"] for i in part}), **gnn_score(net, val), "fit_s": time.time() - s}
        return kind, entry, None
    if kind == "sweep":
        net = gnn_fit(items, settings, f"sweep {settings}")
        entry = {**settings, **gnn_score(net, val), "fit_s": time.time() - s}
        return kind, entry, net.state_dict()
    train, test = fold_items(items, arg)
    cut = sorted({i["layout"] for i in train})[::5]
    fit = [i for i in train if i["layout"] not in cut]
    hold = [i for i in train if i["layout"] in cut]
    net = gnn_fit(fit, settings, arg)
    return kind, {"fold": arg, "train_rows": len(fit), **gnn_score(net, hold, test), "fit_s": time.time() - s}, None


def linear(layer):
    return {"w": layer.weight.detach().double().tolist(), "b": layer.bias.detach().double().tolist()}


def export_gnn(net, settings, temperature, schemas, name):
    from train_gnn import AS_BUILT as GNN_AS_BUILT, KINDS as GNN_KINDS

    return {
        "name": name,
        "architecture": "tree_gnn",
        "observation_schema": schemas["observation_schema"],
        "fault_schema": schemas["fault_schema"],
        "trained_on": "Synthetic",
        "hidden": settings["hidden"],
        "enc": [linear(net.enc[0]), linear(net.enc[2])],
        "rounds": [linear(layer[0]) for layer in net.round],
        "mix": [linear(net.mix[0]), linear(net.mix[2])],
        "kind_embedding": net.kind.weight.detach().double().tolist(),
        "score": [linear(net.score[0]), linear(net.score[2])],
        "temperature": temperature,
        **({"node_kinds": list(GNN_KINDS)} if GNN_AS_BUILT else {}),
        **({"plant_schema": schemas["versions"]["plant"]} if "versions" in schemas else {}),
    }


def write_parity(net, path, rows_file, tensorize, count=4):
    """What Rust has to reproduce, in double precision, so a difference is a bug and not rounding."""
    import copy

    import torch

    double = copy.deepcopy(net).double()
    parity = []
    with torch.no_grad():
        for r in rows_of(rows_file):
            t = tensorize(r, np.float64)
            parity.append({"nodes": r["graph"], "rows": r["candidates"], "scores": double(t).tolist()})
            if len(parity) == count:
                break
    Path(path).write_text(json.dumps(parity))


def gnn_main(out, generated, train_maps, calibrate):
    import torch

    from train_gnn import Net, tensorize

    t0 = time.time()
    SHARED["items"] = gnn_load(generated, "generated", tensorize) + gnn_load(train_maps, "maps", tensorize)
    SHARED["val"] = gnn_load(calibrate, "calibrate", tensorize)
    print(len(SHARED["items"]), "training rows,", len(SHARED["val"]), "calibrate rows,", f"{time.time() - t0:.0f}s", flush=True)
    context = mp.get_context("fork")
    tasks = [("sweep", None, s) for s in GNN_SWEEP] + [("curve", f, GNN_DEFAULT) for f in FRACTIONS[:-1]]
    with context.Pool(7) as pool:
        finished = pool.map(gnn_task, tasks, chunksize=1)
    results = {"origin": "Synthetic", "train_rows": len(SHARED["items"]), "calibrate_rows": len(SHARED["val"])}
    sweep = [(e, state) for kind, e, state in finished if kind == "sweep"]
    results["sweep"] = [e for e, _ in sweep]
    # The curve's last point is the default settings on everything, which the sweep already fitted.
    last = next(e for e, _ in sweep if e["hidden"] == GNN_DEFAULT["hidden"] and e["epochs"] == GNN_DEFAULT["epochs"])
    results["curve"] = [e for kind, e, _ in finished if kind == "curve"] + [{"fraction": 1.0, "rows": len(SHARED["items"]), "layouts": len({i["layout"] for i in SHARED["items"]}), **last}]
    entry, state = min(sweep, key=lambda pair: pair[0]["nll"])
    settings = {"hidden": entry["hidden"], "epochs": entry["epochs"]}
    results["chosen"] = settings
    print("chosen", entry, flush=True)
    net = Net(settings["hidden"])
    net.load_state_dict(state)
    first = SHARED["items"][0]
    schemas = {"observation_schema": first["observation_schema"], "fault_schema": first["fault_schema"], **({"versions": first["versions"]} if "versions" in first else {})}
    (out / "gnn_maps.json").write_text(json.dumps(export_gnn(net, settings, entry["temperature"], schemas, "gnn-maps")))
    write_parity(net, out / "gnn_parity.json", calibrate, tensorize)
    (out / "results_maps_gnn.json").write_text(json.dumps(results, indent=1))


def gnn_folds_main(out, generated, train_maps, _calibrate):
    """A process of its own: gnn_main has run torch by its end, and a fork after that hangs on OpenMP's locks."""
    from train_gnn import tensorize

    settings = json.loads((out / "results_maps_gnn.json").read_text())["chosen"]
    SHARED["items"] = gnn_load(generated, "generated", tensorize) + gnn_load(train_maps, "maps", tensorize)
    SHARED["val"] = []
    with mp.get_context("fork").Pool(5) as pool:
        folds = pool.map(gnn_task, [("fold", f, settings) for f in FOLDS], chunksize=1)
    results = {"origin": "Synthetic", "chosen": settings, "folds": {e["fold"]: e for _, e, _ in folds}}
    (out / "results_maps_gnn_folds.json").write_text(json.dumps(results, indent=1))
    print(json.dumps(results["folds"], indent=1), flush=True)


if __name__ == "__main__":
    which, out = sys.argv[1], Path(sys.argv[2])
    {"gbdt": gbdt_main, "gnn": gnn_main, "gnn-folds": gnn_folds_main}[which](out, *sys.argv[3:])
