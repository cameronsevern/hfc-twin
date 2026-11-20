"""Editing a trained spectrum model's weights by hand (docs/tasks/18-weight-space-editing.md). All synthetic.

    ml/data/venv-gpu/bin/python ml/weight_edit.py inspect ARTIFACT          # tensors, and the channels that fire on the worst captures
    ml/data/venv-gpu/bin/python ml/weight_edit.py errors ARTIFACT           # the rule's parts on calibrate, and the worst captures
    ml/data/venv-gpu/bin/python ml/weight_edit.py apply ARTIFACT EDIT.json  # a new artifact beside it, with the edit in its log
    ml/data/venv-gpu/bin/python ml/weight_edit.py score ARTIFACT            # against its parent, on calibrate: kept or not
    ml/data/venv-gpu/bin/python ml/weight_edit.py heldout PARENT FINAL      # the one paired read on task 15's reserve areas
    ml/data/venv-gpu/bin/python ml/weight_edit.py parity ARTIFACT           # double-precision cases for proposal::spectrum to match

Reads `calibrate` and, once, `reserve`. Never `test`: those maps were read once for task 17 and stay sealed. The artifact's JSON is
the only source of weights, the same file `proposal::spectrum` reads, so nothing scored here can differ from what Rust runs. Nothing
here writes outside `ml/artifacts/edits/`, and nothing calls `evaluate_spectrum.main()`, which would overwrite task 17's files.
"""
import contextlib
import copy
import hashlib
import json
import sys
from pathlib import Path

import numpy as np
import torch

from evaluate_spectrum import BAND_SHAPES, VISIBLE_DB, Truth, choose, marks, score
from train_spectrum import BATCH, SHAPES, Net, cache, inputs_of, wait_for_gpu

ROOT = Path(__file__).resolve().parent.parent
DATA = ROOT / "ml" / "data" / "spectra"
EDITS = ROOT / "ml" / "artifacts" / "edits"
SPLITS = {"calibrate": DATA / "calibrate.jsonl", "reserve": DATA / "reserve.jsonl"}
HELDOUT = EDITS / "heldout.json"
LAYERS = ["stem"] + [f"blocks.{i}" for i in range(9)] + ["mix", "head"]
FALL = len(SHAPES)  # the head's last output: the fall, dB
# What counts as noise, from the task's rule: 0.5 points of a share, 0.01 of an F1 or an IoU, 0.05 dB.
SHARE_NOISE, F1_NOISE, DB_NOISE = 0.005, 0.01, 0.05
FALL_BAR_DB = 2.0
LOWMER_BAR = 0.70
MAX_EDITS = 20


class Refused(Exception):
    pass


def split_path(split):
    if split == "test":
        raise Refused("the test maps are sealed: task 18 reads calibrate, and reserve once")
    if split not in SPLITS:
        raise Refused(f"no split {split!r}")
    return SPLITS[split]


def load(path):
    return json.loads(Path(path).read_text())


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()[:16]


def tensors(model):
    """Every tensor by name, shaped as torch holds it: a conv's `w` is (outputs, inputs, kernel)."""
    out = {}
    for name in LAYERS:
        conv = model[name] if "." not in name else model["blocks"][int(name.split(".")[1])]
        shape = (conv["outputs"], conv["inputs"], conv["kernel"])
        out[f"{name}.w"] = np.asarray(conv["w"], np.float64).reshape(shape)
        out[f"{name}.b"] = np.asarray(conv["b"], np.float64)
    return out


def put(model, name, value):
    layer, part = name.rsplit(".", 1)
    conv = model[layer] if "." not in layer else model["blocks"][int(layer.split(".")[1])]
    conv[part] = np.asarray(value, np.float64).ravel().tolist()


def width_of(model):
    return model["stem"]["outputs"]


def state_of(model, dtype=torch.float32):
    """The artifact's weights as a state dict for `train_spectrum.Net`."""
    state = {}
    for name, value in tensors(model).items():
        layer, part = name.rsplit(".", 1)
        state[f"{layer}.{'weight' if part == 'w' else 'bias'}"] = torch.tensor(value, dtype=dtype)
    return state


def net_of(model, device, dtype=torch.float32):
    net = Net(width_of(model))
    if dtype == torch.float64:
        net = net.double()
    net.load_state_dict(state_of(model, dtype))
    return net.to(device).eval()


def device_of():
    if torch.cuda.is_available():
        # Its wait messages go to stderr, so a command's stdout stays one JSON document.
        with contextlib.redirect_stdout(sys.stderr):
            wait_for_gpu()
        return "cuda"
    return "cpu"


def predict(model, data, device=None, hooks=None):
    """As `evaluate_spectrum.predict`, from the artifact's JSON: probabilities as bytes, falls in dB."""
    device = device or device_of()
    net = net_of(model, device)
    n = len(data["meta"])
    probs = np.zeros((n, len(SHAPES), data["min"].shape[1]), np.uint8)
    falls = np.zeros((n, data["min"].shape[1]), np.float32)
    first = np.array([m["first_mhz"] for m in data["meta"]])
    with torch.no_grad():
        for start in range(0, n, BATCH):
            end = min(start + BATCH, n)
            x = torch.from_numpy(inputs_of(data["min"][start:end], data["mean"][start:end], first[start:end])).to(device)
            out = net(x)
            probs[start:end] = (torch.sigmoid(out[:, : len(SHAPES)]) * 255).byte().cpu().numpy()
            falls[start:end] = out[:, FALL].cpu().numpy()
    return probs, falls


def small_falls(falls, truth):
    """The fall error on present bins that fall less than 3 dB: the side the rule's own fall error doesn't read."""
    pick = truth.present & (truth.fall < VISIBLE_DB)
    return float(np.abs(falls[pick] - truth.fall[pick]).mean()) if pick.any() else 0.0


def fall_bands(falls, truth):
    out = {}
    for lo, hi in [(0, 3), (3, 6), (6, 10), (10, 20), (20, 1000)]:
        pick = truth.present & (truth.fall >= lo) & (truth.fall < hi)
        if pick.any():
            err = falls[pick] - truth.fall[pick]
            out[f"{lo}-{hi}"] = {"bins": int(pick.sum()), "mae_db": float(np.abs(err).mean()), "bias_db": float(err.mean())}
    return out


def read(model, split, data=None, thresholds=None, device=None):
    """The rule's parts on one split: with thresholds chosen there (calibrate) or given (reserve, the parent's held)."""
    data = data or cache(split_path(split))
    truth = Truth(data)
    probs, falls = predict(model, data, device)
    chosen = thresholds or choose(probs, falls, truth)
    result = score(probs, falls, truth, chosen, data["meta"])
    result["small_fall_mae_db"] = small_falls(falls, truth)
    result["fall_bands"] = fall_bands(falls, truth)
    return {"thresholds": chosen, "result": result, "probs": probs, "falls": falls, "truth": truth, "data": data}


def parts(result):
    """The parts of task 17's rule an edit can move, each with its noise and which way is better."""
    out = {
        "severe_10_db": (result["severe_10_db"]["not_normal"], SHARE_NOISE, 1),
        "severe_20_db": (result["severe_20_db"]["not_normal"], SHARE_NOISE, 1),
        "healthy_false_alarm": (result["healthy_false_alarm"], SHARE_NOISE, -1),
        "macro_f1": (result["macro_f1"], F1_NOISE, 1),
        "fall_mae_db": (result["fall_mae_db"], DB_NOISE, -1),
        "small_fall_mae_db": (result["small_fall_mae_db"], DB_NOISE, -1),
    }
    for name, v in result["shapes"].items():
        out[f"f1.{name}"] = (v["f1"], F1_NOISE, 1)
    for name in BAND_SHAPES:
        out[f"band_iou.{name}"] = (result["band_iou_median"][name], F1_NOISE, 1)
    return out


def compare(child, parent):
    """Which parts got worse than their noise, and the fall error's change."""
    c, p = parts(child), parts(parent)
    worse = []
    for name, (value, noise, better) in c.items():
        if (p[name][0] - value) * better > noise:
            worse.append({"part": name, "parent": p[name][0], "edit": value})
    return {"worse": worse, "fall_change_db": c["fall_mae_db"][0] - p["fall_mae_db"][0]}


def meets_bars(result):
    return result["fall_mae_db"] <= FALL_BAR_DB and result["shapes"]["LowMer"]["f1"] >= LOWMER_BAR


def view(tensor, index):
    """A slice of a tensor: per dimension an int, null for all of it, or [start, stop]."""
    index = index or []
    key = tuple(slice(None) if i is None else slice(i[0], i[1]) if isinstance(i, list) else i for i in index)
    return key


def apply_ops(model, ops):
    """Each op is `set`, `scale` or `add` on a slice of a named tensor, or a rank-one `outer` update on its 2-D view."""
    model = copy.deepcopy(model)
    names = tensors(model)
    for op in ops:
        name = op["tensor"]
        if name not in names:
            raise Refused(f"no tensor {name!r}")
        t = names[name]
        if op["op"] == "outer":
            flat = t.reshape(t.shape[0], -1)
            u, v = np.asarray(op["u"], np.float64), np.asarray(op["v"], np.float64)
            if u.shape != (flat.shape[0],) or v.shape != (flat.shape[1],):
                raise Refused(f"outer on {name}: u must be {flat.shape[0]} long and v {flat.shape[1]}")
            flat += op.get("alpha", 1.0) * np.outer(u, v)
        else:
            key = view(t, op.get("index"))
            value = np.asarray(op["value"], np.float64)
            if op["op"] == "set":
                t[key] = value
            elif op["op"] == "scale":
                t[key] = t[key] * value
            elif op["op"] == "add":
                t[key] = t[key] + value
            else:
                raise Refused(f"no op {op['op']!r}")
        if not np.isfinite(t).all():
            raise Refused(f"{name} is no longer finite")
        put(model, name, t)
    return model


def kind_of(ops):
    return "head" if all(op["tensor"].startswith("head.") for op in ops) else "inner"


def weights(model):
    return sum(v.size for v in tensors(model).values())


def apply(parent_path, edit_path, out_path=None, device=None, data=None):
    parent_path, edit_path = Path(parent_path), Path(edit_path)
    parent = load(parent_path)
    edit = load(edit_path)
    if not edit.get("rationale"):
        raise Refused("an edit says why, in words")
    log = parent.get("edit_log", [])
    # Every edit applied counts toward the limit, kept or not, and none is written over.
    applied = sorted(EDITS.glob("edit_[0-9][0-9].json")) if out_path is None else []
    if len(applied) >= MAX_EDITS:
        raise Refused(f"{MAX_EDITS} edits is the task's limit")
    child = apply_ops(parent, edit["ops"])
    if weights(child) != weights(parent):
        raise Refused("an edit changes values, not the network")
    # Thresholds are chosen again on calibrate, as a trained model's are.
    child["thresholds"] = read(child, "calibrate", data=data, device=device)["thresholds"]
    child["parent"] = {"file": parent_path.name, "sha256_16": digest(parent_path)}
    child["edit_log"] = log + [{"ops": edit["ops"], "rationale": edit["rationale"], "kind": kind_of(edit["ops"]), "parent": parent_path.name}]
    out_path = Path(out_path) if out_path else EDITS / f"edit_{len(applied) + 1:02d}.json"
    if out_path.exists():
        raise Refused(f"{out_path.name} exists, and an artifact is never written over")
    out_path.parent.mkdir(parents=True, exist_ok=True)
    out_path.write_text(json.dumps(child))
    return out_path


def parent_of(path):
    model = load(path)
    if "parent" not in model:
        raise Refused(f"{path} has no parent")
    here = Path(path).parent / model["parent"]["file"]
    if not here.exists():
        here = ROOT / "ml" / "artifacts" / model["parent"]["file"]
    if digest(here) != model["parent"]["sha256_16"]:
        raise Refused(f"{here} isn't the file this was edited from")
    return here


def strip(reading):
    return {k: reading[k] for k in ("thresholds", "result")}


def score_cmd(path, device=None, data=None):
    path = Path(path)
    child = load(path)
    parent_file = parent_of(path)
    parent = load(parent_file)
    data = data or cache(split_path("calibrate"))
    mine = read(child, "calibrate", data=data, thresholds=child["thresholds"], device=device)
    theirs = read(parent, "calibrate", data=data, device=device)
    held = read(child, "calibrate", data=data, thresholds=theirs["thresholds"], device=device)
    against = compare(mine["result"], theirs["result"])
    kept = against["fall_change_db"] < 0 and not against["worse"] and mine["result"]["shapes"]["LowMer"]["f1"] >= LOWMER_BAR
    out = {
        "origin": "Synthetic",
        "split": "calibrate",
        "artifact": path.name,
        "parent": parent_file.name,
        "kind": child["edit_log"][-1]["kind"],
        "kept": bool(kept),
        "meets_bars": bool(meets_bars(mine["result"])) and not against["worse"],
        "against_parent": against,
        "against_parent_parents_thresholds": compare(held["result"], theirs["result"]),
        "edit": mine["result"],
        "parent_result": theirs["result"],
    }
    path.with_suffix(".score.json").write_text(json.dumps(out, indent=1))
    return out


def summary(result):
    return {
        "fall_mae_db": round(result["fall_mae_db"], 4),
        "small_fall_mae_db": round(result["small_fall_mae_db"], 4),
        "macro_f1": round(result["macro_f1"], 4),
        "lowmer_f1": round(result["shapes"]["LowMer"]["f1"], 4),
        "min_f1": round(min(v["f1"] for v in result["shapes"].values()), 4),
        "healthy_false_alarm": round(result["healthy_false_alarm"], 4),
        "severe_10_db": round(result["severe_10_db"]["not_normal"], 4),
        "severe_20_db": round(result["severe_20_db"]["not_normal"], 4),
    }


def errors(path, device=None, worst=15):
    model = load(path)
    r = read(model, "calibrate", thresholds=model.get("thresholds"), device=device)
    truth, falls, probs, meta = r["truth"], r["falls"], r["probs"], r["data"]["meta"]
    found, *_ = marks(probs, falls, truth, r["thresholds"])
    s = SHAPES.index("LowMer")
    pos, neg = truth.worst[:, s] >= VISIBLE_DB, truth.worst[:, s] == 0
    hit = truth.present & (truth.fall >= VISIBLE_DB)
    per = np.where(hit, np.abs(falls - truth.fall), 0).sum(axis=1) / np.maximum(hit.sum(axis=1), 1)

    def who(i):
        m = meta[i]
        return {"index": int(i), "map": m["map"], "profile": m["profile"], "causes": m["causes"], "shapes": [x["shape"] for x in m["shapes"]], "worst_fall_db": round(m["worst_fall_db"], 2)}

    missed = np.nonzero(pos & ~found[:, s])[0]
    false = np.nonzero(neg & found[:, s])[0]
    # LowMer misses whose curve is flat and low: task 17's labels let the same curve carry `LowMer` or healthy.
    flat = []
    for i in missed:
        present = truth.present[i]
        flat.append(float(np.std(truth.fall[i][present])) if present.any() else 0.0)
    out = {
        "origin": "Synthetic",
        "split": "calibrate",
        "summary": summary(r["result"]),
        "fall_bands": r["result"]["fall_bands"],
        "shapes": r["result"]["shapes"],
        "lowmer": {
            "missed": int(len(missed)),
            "false_calls": int(len(false)),
            "missed_with_another_shape_named": int(sum(1 for i in missed if found[i].any())),
            "missed_flat_share": float(np.mean(np.asarray(flat) < 1.0)) if flat else None,
            "missed_causes": count(meta[i]["causes"] for i in missed),
            "false_call_causes": count(meta[i]["causes"] for i in false),
            "worst_missed": [who(i) for i in missed[np.argsort(-truth.worst[missed, s])][:worst]],
        },
        "worst_fall": [dict(who(i), mae_db=round(float(per[i]), 2), bias_db=round(float(np.where(hit[i], falls[i] - truth.fall[i], 0).sum() / max(hit[i].sum(), 1)), 2)) for i in np.argsort(-per)[:worst]],
    }
    return out


def count(lists):
    out = {}
    for items in lists:
        for x in items:
            out[x] = out.get(x, 0) + 1
    return dict(sorted(out.items(), key=lambda kv: -kv[1]))


def inspect(path, device=None, worst=200):
    model = load(path)
    device = device or device_of()
    stats = {name: {"shape": list(t.shape), "mean": float(t.mean()), "std": float(t.std()), "max_abs": float(np.abs(t).max())} for name, t in tensors(model).items()}
    data = cache(split_path("calibrate"))
    r = read(model, "calibrate", data=data, thresholds=model.get("thresholds"), device=device)
    truth, falls = r["truth"], r["falls"]
    hit = truth.present & (truth.fall >= VISIBLE_DB)
    per = np.where(hit, np.abs(falls - truth.fall), 0).sum(axis=1) / np.maximum(hit.sum(axis=1), 1)
    pick = np.sort(np.argsort(-per)[:worst])
    rng = np.random.default_rng(0)
    base = np.sort(rng.choice(len(per), min(2000, len(per)), replace=False))
    net = net_of(model, device)
    seen = {}

    def hook(name):
        def f(_, __, out):
            seen[name] = torch.relu(out) if name != "head" else out
        return f

    handles = [dict(net.named_modules())[name].register_forward_hook(hook(name)) for name in LAYERS[:-1]]
    first = np.array([m["first_mhz"] for m in data["meta"]])

    def means(index):
        total = {name: 0.0 for name in LAYERS[:-1]}
        with torch.no_grad():
            for at in range(0, len(index), BATCH):
                i = index[at : at + BATCH]
                x = torch.from_numpy(inputs_of(data["min"][i], data["mean"][i], first[i])).to(device)
                net(x)
                present = x[:, 3:4]
                for name in total:
                    total[name] = total[name] + ((seen[name] * present).sum(dim=-1) / present.sum(dim=-1).clamp(min=1)).sum(dim=0).cpu().numpy()
        return {name: v / len(index) for name, v in total.items()}

    hot, cold = means(pick), means(base)
    for h in handles:
        h.remove()
    layers = {}
    for name in LAYERS[:-1]:
        lift = hot[name] - cold[name]
        top = np.argsort(-np.abs(lift))[:6]
        layers[name] = [{"channel": int(c), "on_worst": round(float(hot[name][c]), 4), "on_all": round(float(cold[name][c]), 4)} for c in top]
    head = tensors(model)["head.w"][FALL, :, 0]
    return {"origin": "Synthetic", "split": "calibrate", "weights": weights(model), "tensors": stats, "worst_fall_channels": layers, "head_fall_row": {"bias": float(tensors(model)["head.b"][FALL]), "largest": [{"channel": int(c), "w": round(float(head[c]), 4)} for c in np.argsort(-np.abs(head))[:10]]}}


def heldout(parent_path, final_path, device=None):
    """The one paired read on the reserve areas: parent and final edit, each with its own calibrate thresholds."""
    if HELDOUT.exists():
        raise Refused(f"the reserve maps have been read once already ({HELDOUT.name})")
    parent, final = load(parent_path), load(final_path)
    data = cache(split_path("reserve"))
    p = read(parent, "reserve", data=data, thresholds=parent["thresholds"], device=device)["result"]
    f = read(final, "reserve", data=data, thresholds=final["thresholds"], device=device)["result"]
    against = compare(f, p)
    out = {
        "origin": "Synthetic",
        "note": "Task 15's three reserve areas, read once. Real streets, invented plant, simulated faults and telemetry.",
        "split": "reserve",
        "parent": Path(parent_path).name,
        "final": Path(final_path).name,
        "parent_result": p,
        "final_result": f,
        "against_parent": against,
        "fall_lower_than_noise": bool(against["fall_change_db"] < -DB_NOISE),
        "nothing_worse": not against["worse"],
    }
    out["holds"] = out["fall_lower_than_noise"] and out["nothing_worse"]
    HELDOUT.write_text(json.dumps(out, indent=1))
    return out


def parity(path, data=None, cases=8):
    """A double-precision run of an artifact's weights on calibrate captures, for `proposal::spectrum` to match to 1e-9."""
    path = Path(path)
    model = load(path)
    data = data or cache(split_path("calibrate"))
    net = net_of(model, "cpu", torch.float64)
    index = np.linspace(0, len(data["meta"]) - 1, cases).astype(int)
    out = []
    for i in index:
        low, mean = np.asarray(data["min"][i]), np.asarray(data["mean"][i])
        first = data["meta"][i]["first_mhz"]
        x = inputs_of(low, mean, first, np.float64)
        with torch.no_grad():
            y = net(torch.from_numpy(x)[None])[0].numpy()
        out.append({"first_mhz": first, "min": low.tolist(), "mean": mean.tolist(), "outputs": y.tolist()})
    target = path.with_suffix(".parity.json")
    target.write_text(json.dumps({"origin": "Synthetic", "artifact": path.name, "cases": out}))
    return target


def main():
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    command, args = sys.argv[1], sys.argv[2:]
    if command == "inspect":
        out = inspect(args[0])
    elif command == "errors":
        out = errors(args[0])
    elif command == "apply":
        out = {"wrote": str(apply(args[0], args[1], args[2] if len(args) > 2 else None))}
    elif command == "score":
        s = score_cmd(args[0])
        out = {k: s[k] for k in ("artifact", "parent", "kind", "kept", "meets_bars", "against_parent", "against_parent_parents_thresholds")}
        out["edit"], out["parent_result"] = summary(s["edit"]), summary(s["parent_result"])
    elif command == "heldout":
        h = heldout(args[0], args[1])
        out = {k: h[k] for k in ("against_parent", "fall_lower_than_noise", "nothing_worse", "holds")}
        out["parent"], out["final"] = summary(h["parent_result"]), summary(h["final_result"])
    elif command == "parity":
        out = {"wrote": str(parity(args[0]))}
    else:
        raise SystemExit(__doc__)
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    try:
        main()
    except Refused as e:
        raise SystemExit(f"refused: {e}")
