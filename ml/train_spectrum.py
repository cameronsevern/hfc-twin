"""Trains the spectrum CNN (docs/tasks/17-spectrum-model.md) on rows Rust wrote. Never reads the test maps. All synthetic.

    ml/data/venv-gpu/bin/python ml/train_spectrum.py <data dir> <artifacts dir>

Waits for GPU memory rather than taking it from whatever else is running, and picks up from its last epoch if the GPU fills mid-fit.
"""
import json
import subprocess
import sys
import time
from pathlib import Path

import numpy as np
import torch
from torch import nn

SHAPES = ["Ingress", "Spike", "Dip", "RollOff", "BandLoss", "Ripple", "Wave", "LowMer", "Edge"]
BINS = 960
BIN_MHZ = 0.2
PADDING = 255
DILATIONS = [1, 2, 4, 8, 16, 32, 64, 128, 256]
INPUTS = 4
TOP_MHZ = 1218.0
GPU_NEEDED_MIB = 2500
SWEEP = [{"width": 48}, {"width": 64}]
EPOCHS = 40
BATCH = 256
WEIGHT_LIMIT = 200_000


def wait_for_gpu():
    while True:
        out = subprocess.run(["nvidia-smi", "--query-gpu=memory.free", "--format=csv,noheader,nounits"], capture_output=True, text=True).stdout
        free = int(out.split()[0])
        if free >= GPU_NEEDED_MIB:
            print(f"GPU has {free} MiB free", flush=True)
            return
        print(f"waiting for the GPU: {free} MiB free, need {GPU_NEEDED_MIB}", flush=True)
        time.sleep(120)


def cache(path):
    """Rust's rows as arrays, once."""
    path = Path(path)
    base = path.with_suffix("")
    if not Path(f"{base}.meta.json").exists():
        low, mean, fall, bits, meta = [], [], [], [], []
        for line in open(path):
            if not line.strip():
                continue
            r = json.loads(line)
            if r["origin"] != "Synthetic":
                raise SystemExit("task 17 trains on synthetic rows only")
            low.append(np.frombuffer(bytes.fromhex(r["min_hex"]), np.uint8))
            mean.append(np.frombuffer(bytes.fromhex(r["mean_hex"]), np.uint8))
            fall.append(np.frombuffer(bytes.fromhex(r["fall_hex"]), np.uint8))
            bits.append(np.frombuffer(bytes.fromhex(r["shapes_hex"]), "<u2"))
            meta.append({k: r[k] for k in ("map", "profile", "first_mhz", "worst_fall_db", "shapes", "causes", "observation_schema", "fault_schema", "schema_version", "service_group")})
        for name, data in (("min", low), ("mean", mean), ("fall", fall), ("bits", bits)):
            np.save(f"{base}.{name}.npy", np.stack(data))
        Path(f"{base}.meta.json").write_text(json.dumps(meta))
    return {
        "min": np.load(f"{base}.min.npy", mmap_mode="r"),
        "mean": np.load(f"{base}.mean.npy", mmap_mode="r"),
        "fall": np.load(f"{base}.fall.npy", mmap_mode="r"),
        "bits": np.load(f"{base}.bits.npy", mmap_mode="r"),
        "meta": json.loads(Path(f"{base}.meta.json").read_text()),
    }


def inputs_of(low, mean, first_mhz, dtype=np.float32):
    """The channels the network reads. `proposal::spectrum::input_channels` is the same, and the parity file holds them to it."""
    low, mean = np.asarray(low), np.asarray(mean)
    present = mean != PADDING
    count = np.maximum(present.sum(axis=-1, keepdims=True), 1)
    overall = (mean * present).sum(axis=-1, keepdims=True).astype(np.float64) / count
    f = np.asarray(first_mhz, dtype=np.float64)[..., None] + np.arange(BINS) * BIN_MHZ
    return np.stack(
        [
            np.where(present, low, overall) / 4.0 / 50.0,
            np.where(present, mean, overall) / 4.0 / 50.0,
            f / TOP_MHZ,
            present.astype(np.float64),
        ],
        axis=-2,
    ).astype(dtype)


class Net(nn.Module):
    def __init__(self, width):
        super().__init__()
        self.stem = nn.Conv1d(INPUTS, width, 3, padding=1)
        self.blocks = nn.ModuleList(nn.Conv1d(width, width, 3, padding=d, dilation=d) for d in DILATIONS)
        self.mix = nn.Conv1d(2 * width, width, 1)
        self.head = nn.Conv1d(width, len(SHAPES) + 1, 1)

    def forward(self, x):
        h = torch.relu(self.stem(x))
        for block in self.blocks:
            h = h + torch.relu(block(h))
        # A flat fall has no edge for a convolution to find, so every bin also sees a summary of the whole channel, padding left out.
        present = x[:, 3:4]
        summary = (h * present).sum(dim=-1, keepdim=True) / present.sum(dim=-1, keepdim=True).clamp(min=1)
        h = h + torch.relu(self.mix(torch.cat([h, summary.expand_as(h)], dim=1)))
        return self.head(h)


def batch(data, index, device):
    index = np.sort(index)
    first = np.array([data["meta"][i]["first_mhz"] for i in index])
    x = torch.from_numpy(inputs_of(data["min"][index], data["mean"][index], first)).to(device)
    present = x[:, 3]
    fall = torch.from_numpy(np.asarray(data["fall"][index], np.float32) / 4.0).to(device)
    bits = torch.from_numpy(np.asarray(data["bits"][index], np.int64)).to(device)
    y = torch.stack([((bits >> k) & 1).float() for k in range(len(SHAPES))], dim=1)
    return x, y, fall, present


def loss_of(out, y, fall, present):
    shape = nn.functional.binary_cross_entropy_with_logits(out[:, : len(SHAPES)], y, reduction="none", pos_weight=torch.full((len(SHAPES), 1), 4.0, device=y.device)).mean(dim=1)
    size = nn.functional.huber_loss(out[:, len(SHAPES)], fall, reduction="none", delta=3.0)
    denominator = present.sum().clamp(min=1)
    return (shape * present).sum() / denominator + 0.05 * (size * present).sum() / denominator


def validate(net, data, device):
    net.eval()
    total, n = 0.0, 0
    with torch.no_grad():
        for start in range(0, len(data["meta"]), BATCH):
            index = np.arange(start, min(start + BATCH, len(data["meta"])))
            x, y, fall, present = batch(data, index, device)
            total += float(loss_of(net(x), y, fall, present)) * len(index)
            n += len(index)
    return total / max(n, 1)


def fit(data, valid, settings, label, out_dir, keep=None, epochs=EPOCHS):
    """One fit, checkpointed each epoch, so a full GPU costs a wait and not the run."""
    device = "cuda"
    path = out_dir / f"{label}.ckpt.pt"
    torch.manual_seed(0)
    net = Net(settings["width"])
    weights = sum(p.numel() for p in net.parameters())
    if weights > WEIGHT_LIMIT:
        raise SystemExit(f"{weights} weights is past the limit")
    start = 0
    state = None
    if path.exists():
        state = torch.load(path, map_location="cpu")
        net.load_state_dict(state["net"])
        start = state["epoch"] + 1
    index = np.arange(len(data["meta"])) if keep is None else np.asarray(keep)
    rng = np.random.default_rng(0)
    orders = [rng.permutation(index) for _ in range(epochs)]
    print(label, weights, "weights", len(index), "captures, from epoch", start, flush=True)
    while start < epochs:
        try:
            wait_for_gpu()
            net.to(device)
            optimizer = torch.optim.Adam(net.parameters(), lr=1e-3)
            scheduler = torch.optim.lr_scheduler.CosineAnnealingLR(optimizer, epochs)
            if state is not None:
                optimizer.load_state_dict(state["optimizer"])
                scheduler.load_state_dict(state["scheduler"])
            for epoch in range(start, epochs):
                t0 = time.time()
                net.train()
                order = orders[epoch]
                for at in range(0, len(order), BATCH):
                    x, y, fall, present = batch(data, order[at : at + BATCH], device)
                    optimizer.zero_grad()
                    loss = loss_of(net(x), y, fall, present)
                    loss.backward()
                    optimizer.step()
                scheduler.step()
                score = validate(net, valid, device)
                state = {"net": {k: v.cpu() for k, v in net.state_dict().items()}, "optimizer": optimizer.state_dict(), "scheduler": scheduler.state_dict(), "epoch": epoch, "valid": score}
                torch.save(state, path)
                start = epoch + 1
                print(f"{label} epoch {epoch} valid {score:.4f} {time.time() - t0:.0f}s", flush=True)
        except torch.OutOfMemoryError:
            print("the GPU filled up, waiting", flush=True)
            net.cpu()
            torch.cuda.empty_cache()
            time.sleep(120)
    return net.cpu(), state["valid"] if state else validate(net, valid, device)


def conv_json(conv):
    return {
        "kernel": conv.kernel_size[0],
        "dilation": conv.dilation[0],
        "w": conv.weight.detach().double().numpy().ravel().tolist(),
        "b": conv.bias.detach().double().numpy().tolist(),
        "inputs": conv.in_channels,
        "outputs": conv.out_channels,
    }


def main():
    data_dir, artifacts = Path(sys.argv[1]), Path(sys.argv[2])
    work = data_dir / "fits"
    work.mkdir(parents=True, exist_ok=True)
    train = cache(data_dir / "train.jsonl")
    valid = cache(data_dir / "calibrate.jsonl")
    print(len(train["meta"]), "training captures", len(valid["meta"]), "calibration captures", flush=True)

    # A quarter of the data, whole nodes, picks the width. Then the full fit.
    groups = np.array([hash_group(m) for m in train["meta"]])
    quarter = np.nonzero(groups % 4 == 0)[0]
    results = {}
    for s in SWEEP:
        _, score = fit(train, valid, s, f"sweep_{s['width']}", work, keep=quarter, epochs=8)
        results[s["width"]] = score
    width = min(results, key=results.get)
    print("sweep", results, "chose", width, flush=True)

    net, score = fit(train, valid, {"width": width}, "final", work)
    torch.save(net.state_dict(), work / "final.pt")

    # A separate fit that never saw `BandLoss`, for the "says don't know" part of the rule.
    bit = 1 << SHAPES.index("BandLoss")
    no_band = np.nonzero((np.asarray(train["bits"]) & bit).max(axis=1) == 0)[0]
    blind, _ = fit(train, valid, {"width": width}, "no_bandloss", work, keep=no_band)
    torch.save(blind.state_dict(), work / "no_bandloss.pt")

    model = {
        "name": "spectrum_cnn",
        "architecture": "dilated-cnn-2",
        "origin": "Synthetic",
        "observation_schema": train["meta"][0]["observation_schema"],
        "fault_schema": train["meta"][0]["fault_schema"],
        "spectrum_schema": train["meta"][0]["schema_version"],
        "trained_on": {"captures": len(train["meta"]), "split": "train maps, task 15"},
        "settings": {"width": width, "epochs": EPOCHS, "sweep_valid_loss": results},
        "shapes": SHAPES,
        "stem": conv_json(net.stem),
        "blocks": [conv_json(b) for b in net.blocks],
        "mix": conv_json(net.mix),
        "head": conv_json(net.head),
    }
    (artifacts / "spectrum_model.json").write_text(json.dumps(model))

    # Parity: a double-precision run of these weights, for Rust to match.
    double = Net(width).double()
    double.load_state_dict({k: v.double() for k, v in net.state_dict().items()})
    double.eval()
    index = np.linspace(0, len(valid["meta"]) - 1, 8).astype(int)
    cases = []
    for i in index:
        low, mean = np.asarray(valid["min"][i]), np.asarray(valid["mean"][i])
        first = valid["meta"][i]["first_mhz"]
        x = inputs_of(low, mean, first, np.float64)
        with torch.no_grad():
            y = double(torch.from_numpy(x)[None])[0].numpy()
        cases.append({"first_mhz": first, "min": low.tolist(), "mean": mean.tolist(), "outputs": y.tolist()})
    (artifacts / "spectrum_parity.json").write_text(json.dumps({"origin": "Synthetic", "cases": cases}))
    print("done", score, flush=True)


def hash_group(meta):
    """Stable across runs, unlike Python's own hash."""
    total = 0
    for c in f'{meta["map"]}/{meta["profile"]}/{meta["service_group"]}'.encode():
        total = (total * 131 + c) % 1_000_003
    return total


if __name__ == "__main__":
    main()
