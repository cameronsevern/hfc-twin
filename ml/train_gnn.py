"""The graph candidate: a network over the plant tree, on the same folds as train.py. Reads Rust's files only (invariant 6).

    .venv/bin/python ml/train_gnn.py ml/data/rows.jsonl ml/artifacts [epochs]

Modems encode their own telemetry, messages run along the tree, and each candidate reads what's below it and what isn't. Comparison only:
it ships only if it wins, since exporting it means writing its forward pass in Rust.
"""
import json
import multiprocessing as mp
import sys
import time
from pathlib import Path

import numpy as np
import torch
from torch import nn

from train import FOLDS, component_metrics, fit_temperature, fold_rows, load, split_val, truth_component

import os

# Task 24: on version 2 plant the graph has its new element kinds and fittings, and candidates two new kinds. The lists are
# `proposal::graph::NODE_KINDS` and `proposal::rows::ComponentKind`'s order; a model records the node kinds it reads.
AS_BUILT = os.environ.get("HFC_AS_BUILT") == "1"
KINDS = (
    ["modem", "amp", "root", "tap", "other", "ground_block", "drop_amp", "filter", "cpe", "line_eq", "fitting"]
    if AS_BUILT
    else ["modem", "amp", "root", "tap", "other"]
)
CANDIDATE_KINDS = ["Healthy", "Span", "Amp", "Drop", "Supply", "Root"] + (["Premises", "DropCable"] if AS_BUILT else [])
WIDTH = 21  # 13 modem measures and 8 coarse level buckets, as datagen writes them.
GLOBAL = 11  # The service group's own features, which belong to no node.
HIDDEN = 64


def tensorize(r, dtype=np.float32):
    nodes = r["graph"]
    n = len(nodes)
    x = np.zeros((n, WIDTH * 2 + len(KINDS) + 1), dtype=dtype)
    for i, node in enumerate(nodes):
        for j, v in enumerate(node["x"][:WIDTH]):
            if v is not None:
                x[i, j] = float(np.clip(v, -50, 50)) / 10.0
                x[i, WIDTH + j] = 1.0
        kind = node["kind"] if node["kind"] in KINDS else "other"
        x[i, WIDTH * 2 + KINDS.index(kind)] = 1.0
        x[i, -1] = np.log1p(node["len_ft"]) / 8.0
    parent = np.array([node["parent"] for node in nodes], dtype=np.int64)
    # Which modems lie below each element, itself included.
    modems = [i for i, node in enumerate(nodes) if node["kind"] == "modem"]
    pairs = []
    for m in modems:
        at = m
        while at >= 0:
            pairs.append((at, m))
            at = parent[at]
    pairs = np.array(pairs, dtype=np.int64).reshape(-1, 2)
    root = next(i for i, node in enumerate(nodes) if node["parent"] < 0)
    by_id = {node["id"]: i for i, node in enumerate(nodes)}
    by_edge = {node["edge"]: i for i, node in enumerate(nodes) if node["edge"]}
    idx, kinds = [], []
    for c in r["candidates"]:
        kind = c["kind"]
        if kind in ("Span", "DropCable"):
            idx.append(by_edge[c["component"]])
        elif kind in ("Amp", "Drop"):
            idx.append(by_id[c["component"]])
        elif kind == "Premises":
            idx.append(by_id[c["component"]] if c["component"] in by_id else by_edge[c["component"]])
        else:
            idx.append(root)
        kinds.append(CANDIDATE_KINDS.index(kind))
    g = r["candidates"][0]["x"][:GLOBAL]
    g = np.array([0.0 if v is None else float(np.clip(v, -50, 50)) / 10.0 for v in g], dtype=dtype)
    truth = truth_component(r)
    y = next((i for i, c in enumerate(r["candidates"]) if c["component"] == truth), None)
    return {
        "x": torch.from_numpy(x),
        "parent": torch.from_numpy(parent),
        "pairs": torch.from_numpy(pairs),
        "modems": torch.tensor(modems, dtype=torch.long),
        "idx": torch.tensor(idx, dtype=torch.long),
        "kinds": torch.tensor(kinds, dtype=torch.long),
        "g": torch.from_numpy(g),
        "y": y,
    }


class Net(nn.Module):
    def __init__(self, hidden=HIDDEN):
        super().__init__()
        d = WIDTH * 2 + len(KINDS) + 1
        self.enc = nn.Sequential(nn.Linear(d, hidden), nn.ReLU(), nn.Linear(hidden, hidden))
        self.round = nn.ModuleList(nn.Sequential(nn.Linear(hidden * 3, hidden), nn.ReLU()) for _ in range(3))
        self.mix = nn.Sequential(nn.Linear(hidden * 4 + GLOBAL, hidden), nn.ReLU(), nn.Linear(hidden, hidden), nn.ReLU())
        self.kind = nn.Embedding(len(CANDIDATE_KINDS), 16)
        self.score = nn.Sequential(nn.Linear(hidden + 16, hidden), nn.ReLU(), nn.Linear(hidden, 1))

    def forward(self, t):
        h = self.enc(t["x"])
        n = h.shape[0]
        has_parent = t["parent"] >= 0
        child = torch.nonzero(has_parent).squeeze(1)
        up = t["parent"][child]
        for layer in self.round:
            counts = h.new_zeros(n).index_add_(0, up, h.new_ones(len(child))).clamp(min=1).unsqueeze(1)
            from_children = torch.zeros_like(h).index_add_(0, up, h[child]) / counts
            from_parent = torch.zeros_like(h)
            from_parent[child] = h[up]
            h = h + layer(torch.cat([h, from_children, from_parent], dim=1))
        # What's below each element, and what isn't: the question a fault on it asks.
        at, modem = t["pairs"][:, 0], t["pairs"][:, 1]
        below_sum = torch.zeros_like(h).index_add_(0, at, h[modem])
        below_n = h.new_zeros(n).index_add_(0, at, h.new_ones(len(at))).unsqueeze(1)
        total = h[t["modems"]].sum(0, keepdim=True)
        m = max(len(t["modems"]), 1)
        below = below_sum / below_n.clamp(min=1)
        outside = (total - below_sum) / (m - below_n).clamp(min=1)
        everywhere = (total / m).expand(n, -1)
        z = self.mix(torch.cat([h, below, outside, everywhere, t["g"].expand(n, -1)], dim=1))
        return self.score(torch.cat([z[t["idx"]], self.kind(t["kinds"])], dim=1)).squeeze(1)


def scores_of(net, tensors):
    with torch.no_grad():
        return [net(t).numpy() for t in tensors]


def fit_t(scores, ys):
    flat = np.concatenate(scores)
    groups = np.concatenate([np.full(len(s), i) for i, s in enumerate(scores)])
    labels = np.concatenate([np.eye(len(s), dtype=int)[y] for s, y in zip(scores, ys)])
    return fit_temperature(flat, groups, labels, len(scores))


def ranked(rows, scores, temperature):
    names, conf = [], []
    for r, s in zip(rows, scores):
        p = s / temperature
        p = np.exp(p - p.max())
        p = p / p.sum()
        order = np.argsort(-p, kind="stable")
        names.append([r["candidates"][i]["component"] for i in order])
        conf.append(float(p[order[0]]))
    return names, conf


def run_fold(args):
    name, data, epochs = args
    torch.set_num_threads(3)
    torch.manual_seed(1)
    rows = load(data)
    is_test = dict(FOLDS)[name]
    test, pool = fold_rows(rows, name, is_test)
    train, val = split_val(pool)
    t0 = time.time()
    tr = [tensorize(r) for r in train]
    va = [tensorize(r) for r in val]
    te = [tensorize(r) for r in test]
    net = Net()
    opt = torch.optim.Adam(net.parameters(), lr=2e-3)
    rng = np.random.default_rng(1)
    loss_fn = nn.CrossEntropyLoss()
    for epoch in range(epochs):
        order = rng.permutation(len(tr))
        opt.param_groups[0]["lr"] = 2e-3 * (0.5 ** (epoch // 3))
        for start in range(0, len(order), 16):
            opt.zero_grad()
            for i in order[start : start + 16]:
                t = tr[i]
                (loss_fn(net(t).unsqueeze(0), torch.tensor([t["y"]])) / 16).backward()
            opt.step()
        print(name, "epoch", epoch, f"{time.time() - t0:.0f}s", flush=True)
    temperature = fit_t(scores_of(net, va), [t["y"] for t in va])
    fit_s = time.time() - t0
    s = scores_of(net, te)
    metrics = component_metrics(test, *ranked(test, s, temperature))
    floor_rows = [(r, sc) for r, sc in zip(test, s) if r["floor"]]
    same = component_metrics([r for r, _ in floor_rows], *ranked([r for r, _ in floor_rows], [sc for _, sc in floor_rows], temperature)) if floor_rows else None
    t1 = time.time()
    with torch.no_grad():
        for t in te[:50]:
            net(t)
    per_group_ms = (time.time() - t1) / max(min(50, len(te)), 1) * 1000
    return name, {
        "gnn": {**metrics, "temperature": temperature, "fit_s": fit_s, "per_group_ms": per_group_ms},
        "gnn_on_floor_rows": same,
    }


def main():
    data, out = sys.argv[1], Path(sys.argv[2])
    epochs = int(sys.argv[3]) if len(sys.argv) > 3 else 8
    with mp.get_context("spawn").Pool(5) as pool:
        results = dict(pool.map(run_fold, [(name, data, epochs) for name, _ in FOLDS]))
    (out / "results_gnn.json").write_text(json.dumps({"origin": "Synthetic", "folds": results}, indent=1))
    for name, r in results.items():
        print(name, json.dumps(r["gnn"]), "\n   same rows:", r["gnn_on_floor_rows"])


if __name__ == "__main__":
    main()
