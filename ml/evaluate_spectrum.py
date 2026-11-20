"""The decision rule in docs/tasks/17-spectrum-model.md. All synthetic.

    ml/data/venv-gpu/bin/python ml/evaluate_spectrum.py <data dir> <artifacts dir> calibrate   # chooses thresholds, never reads the test maps
    ml/data/venv-gpu/bin/python ml/evaluate_spectrum.py <data dir> <artifacts dir> test        # applies them, once
"""
import json
import sys
from collections import defaultdict
from pathlib import Path

import numpy as np
import torch

from train_spectrum import BATCH, BINS, SHAPES, Net, cache, inputs_of, wait_for_gpu

VISIBLE_DB = 3.0
SEVERE_BINS = 5
PROBABILITIES = [0.3, 0.5, 0.7, 0.9]
SHAPE_BINS = [1, 3, 5, 10, 20, 40]
UNKNOWN_FALLS = [3.0, 4.0, 5.0, 6.0, 8.0, 10.0]
UNKNOWN_BINS = [5, 10, 20, 40]
HEALTHY_TARGET = 0.03
BAND_SHAPES = ["Ingress", "Dip", "BandLoss", "RollOff"]


def predict(state, width, data):
    device = "cuda"
    net = Net(width)
    net.load_state_dict(state)
    wait_for_gpu()
    net.to(device).eval()
    n = len(data["meta"])
    probs = np.zeros((n, len(SHAPES), BINS), np.uint8)
    falls = np.zeros((n, BINS), np.float32)
    first = np.array([m["first_mhz"] for m in data["meta"]])
    with torch.no_grad():
        for start in range(0, n, BATCH):
            end = min(start + BATCH, n)
            x = torch.from_numpy(inputs_of(data["min"][start:end], data["mean"][start:end], first[start:end])).to(device)
            out = net(x)
            probs[start:end] = (torch.sigmoid(out[:, : len(SHAPES)]) * 255).byte().cpu().numpy()
            falls[start:end] = out[:, len(SHAPES)].cpu().numpy()
    return probs, falls


class Truth:
    def __init__(self, data):
        self.present = np.asarray(data["mean"]) != 255
        self.fall = np.asarray(data["fall"], np.float32) / 4.0
        self.bits = np.asarray(data["bits"])
        n = len(data["meta"])
        self.worst = np.zeros((n, len(SHAPES)))
        for i, meta in enumerate(data["meta"]):
            for label in meta["shapes"]:
                self.worst[i, SHAPES.index(label["shape"])] = label["worst_fall_db"]
        self.healthy = (self.worst.max(axis=1) < 1.5) & (self.fall.max(axis=1) < 1.5)


def marks(probs, falls, truth, thresholds):
    """What the model calls present, per shape and capture, and whether it calls the capture normal."""
    n = len(probs)
    found = np.zeros((n, len(SHAPES)), bool)
    marked = np.zeros(probs.shape, bool)
    for s in range(len(SHAPES)):
        marked[:, s] = (probs[:, s] >= thresholds["probability"][s] * 255) & truth.present
        found[:, s] = marked[:, s].sum(axis=1) >= thresholds["bins"][s]
    named = (marked & found[:, :, None]).any(axis=1)
    unknown = (((falls >= thresholds["unknown_fall_db"]) & truth.present & ~named).sum(axis=1)) >= thresholds["unknown_bins"]
    return found, marked, unknown, ~(found.any(axis=1) | unknown)


def f1(found, truth, s):
    """On captures where the shape falls at least 3 dB, against those where it isn't there at all."""
    pos = truth.worst[:, s] >= VISIBLE_DB
    neg = truth.worst[:, s] == 0
    tp = (found[pos, s]).sum()
    fp = (found[neg, s]).sum()
    fn = (~found[pos, s]).sum()
    precision = tp / max(tp + fp, 1)
    recall = tp / max(tp + fn, 1)
    return {"f1": float(2 * precision * recall / max(precision + recall, 1e-9)), "precision": float(precision), "recall": float(recall), "captures": int(pos.sum())}


def choose(probs, falls, truth):
    thresholds = {"probability": [0.5] * len(SHAPES), "bins": [1] * len(SHAPES), "unknown_fall_db": 10.0, "unknown_bins": 40}
    for s in range(len(SHAPES)):
        best = -1.0
        for p in PROBABILITIES:
            count = ((probs[:, s] >= p * 255) & truth.present).sum(axis=1)
            pos = truth.worst[:, s] >= VISIBLE_DB
            neg = truth.worst[:, s] == 0
            for b in SHAPE_BINS:
                tp, fp, fn = (count[pos] >= b).sum(), (count[neg] >= b).sum(), (count[pos] < b).sum()
                score = 2 * tp / max(2 * tp + fp + fn, 1)
                if score > best:
                    best = score
                    thresholds["probability"][s], thresholds["bins"][s] = p, b
    # The most sensitive "don't know" that stays quiet enough on healthy captures.
    done = False
    for fall in UNKNOWN_FALLS:
        for bins in UNKNOWN_BINS:
            trial = {**thresholds, "unknown_fall_db": fall, "unknown_bins": bins}
            *_, normal = marks(probs, falls, truth, trial)
            if (~normal[truth.healthy]).mean() <= HEALTHY_TARGET:
                thresholds = trial
                done = True
                break
        if done:
            break
    return thresholds


def score(probs, falls, truth, thresholds, meta):
    found, marked, unknown, normal = marks(probs, falls, truth, thresholds)
    severe_bins = (truth.fall >= 10).sum(axis=1)
    worse_bins = (truth.fall >= 20).sum(axis=1)
    severe10, severe20 = severe_bins >= SEVERE_BINS, worse_bins >= SEVERE_BINS
    shapes = {name: f1(found, truth, s) for s, name in enumerate(SHAPES)}
    bands = {}
    for name in BAND_SHAPES:
        s = SHAPES.index(name)
        rows = np.nonzero(truth.worst[:, s] >= VISIBLE_DB)[0]
        real = ((truth.bits[rows] >> s) & 1).astype(bool) & truth.present[rows]
        got = marked[rows, s] & found[rows, s, None]
        inter, union = (real & got).sum(axis=1), (real | got).sum(axis=1)
        bands[name] = float(np.median(inter / np.maximum(union, 1))) if len(rows) else 0.0
    hit = (truth.fall >= VISIBLE_DB) & truth.present
    mae = float(np.abs(falls[hit] - truth.fall[hit]).mean()) if hit.any() else 0.0
    result = {
        "captures": len(meta),
        "healthy_captures": int(truth.healthy.sum()),
        "severe_10_db": {"captures": int(severe10.sum()), "not_normal": float((~normal[severe10]).mean())},
        "severe_20_db": {"captures": int(severe20.sum()), "not_normal": float((~normal[severe20]).mean())},
        "healthy_false_alarm": float((~normal[truth.healthy]).mean()),
        "macro_f1": float(np.mean([v["f1"] for v in shapes.values()])),
        "shapes": shapes,
        "band_iou_median": bands,
        "fall_mae_db": mae,
        "unknown_share": float(unknown.mean()),
    }
    # Read-outs beside the rule.
    bands_of_fall = {}
    worst_any = truth.fall.max(axis=1)
    for lo, hi in [(1.5, 3), (3, 6), (6, 10), (10, 20), (20, 30), (30, 1000)]:
        pick = (worst_any >= lo) & (worst_any < hi)
        bands_of_fall[f"{lo}-{hi}"] = {"captures": int(pick.sum()), "not_normal": float((~normal[pick]).mean()) if pick.any() else None}
    result["not_normal_by_worst_fall"] = bands_of_fall
    for key, field in (("by_map", "map"), ("by_profile", "profile")):
        groups = defaultdict(list)
        for i, m in enumerate(meta):
            groups[m[field]].append(i)
        out = {}
        for name, rows in sorted(groups.items()):
            rows = np.array(rows)
            sev = severe10[rows]
            hl = truth.healthy[rows]
            out[name] = {"captures": len(rows), "severe_not_normal": float((~normal[rows][sev]).mean()) if sev.any() else None, "healthy_false_alarm": float((~normal[rows][hl]).mean()) if hl.any() else None}
        result[key] = out
    visible = (truth.worst >= VISIBLE_DB).sum(axis=1)
    multi = {}
    for label, pick in (("single", visible == 1), ("several", visible >= 2)):
        rows = np.nonzero(pick)[0]
        right = [(found[i, truth.worst[i] >= VISIBLE_DB]).all() for i in rows]
        multi[label] = {"captures": len(rows), "all_visible_shapes_named": float(np.mean(right)) if len(rows) else None}
    result["several_at_once"] = multi
    return result


def main():
    data_dir, artifacts, stage = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3]
    model_path = artifacts / "spectrum_model.json"
    model = json.loads(model_path.read_text())
    width = model["settings"]["width"]
    state = torch.load(data_dir / "fits" / "final.pt", map_location="cpu")
    weights = sum(v.numel() for v in state.values())
    if stage == "calibrate":
        data = cache(data_dir / "calibrate.jsonl")
        truth = Truth(data)
        probs, falls = predict(state, width, data)
        thresholds = choose(probs, falls, truth)
        model["thresholds"] = thresholds
        model_path.write_text(json.dumps(model))
        result = {"origin": "Synthetic", "weights": weights, "thresholds": thresholds, "calibrate": score(probs, falls, truth, thresholds, data["meta"])}
        # A separate fit that never saw a band missing: does it still call those captures not normal?
        blind_state = torch.load(data_dir / "fits" / "no_bandloss.pt", map_location="cpu")
        bp, bf = predict(blind_state, width, data)
        blind_thresholds = choose(bp, bf, truth)
        *_, blind_normal = marks(bp, bf, truth, blind_thresholds)
        s = SHAPES.index("BandLoss")
        pick = truth.worst[:, s] >= 10
        result["says_dont_know"] = {"captures": int(pick.sum()), "not_normal": float((~blind_normal[pick]).mean()), "healthy_false_alarm": float((~blind_normal[truth.healthy]).mean())}
        (artifacts / "results_spectrum_calibrate.json").write_text(json.dumps(result, indent=1))
        print(json.dumps(result, indent=1))
    else:
        data = cache(data_dir / "test.jsonl")
        truth = Truth(data)
        probs, falls = predict(state, width, data)
        calibrate = json.loads((artifacts / "results_spectrum_calibrate.json").read_text())
        result = score(probs, falls, truth, model["thresholds"], data["meta"])
        shape_floor = min(v["f1"] for v in result["shapes"].values())
        latency = json.loads((artifacts / "spectrum_latency.json").read_text())
        rule = {
            "catches_10_db": result["severe_10_db"]["not_normal"] >= 0.99,
            "catches_20_db": result["severe_20_db"]["not_normal"] >= 0.995,
            "quiet_when_healthy": result["healthy_false_alarm"] <= 0.05,
            "macro_f1": result["macro_f1"] >= 0.85,
            "no_shape_under_0.7": shape_floor >= 0.70,
            "finds_the_band": all(v >= 0.7 for v in result["band_iou_median"].values()),
            "sizes_it": result["fall_mae_db"] <= 2.0,
            "says_dont_know": calibrate["says_dont_know"]["not_normal"] >= 0.80,
            "runs_under_50_ms": latency["ms"] < 50.0,
            "under_200k_weights": weights < 200_000,
        }
        out = {"origin": "Synthetic", "note": "Real streets, invented plant, simulated faults and telemetry. A score here says how well the model reads this sim.", "weights": weights, "rule": rule, "ready": all(rule.values()), "test": result, "latency": latency}
        (artifacts / "results_spectrum.json").write_text(json.dumps(out, indent=1))
        print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
