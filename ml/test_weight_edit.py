"""Tests for ml/weight_edit.py on a toy artifact, on the CPU, reading no real data.

    ml/data/venv-gpu/bin/python -m unittest ml/test_weight_edit.py
"""
import json
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np
import torch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import weight_edit as we  # noqa: E402
from train_spectrum import BINS, DILATIONS, INPUTS, SHAPES, Net  # noqa: E402


def conv_json(conv):
    return {
        "kernel": conv.kernel_size[0],
        "dilation": conv.dilation[0],
        "w": conv.weight.detach().double().numpy().ravel().tolist(),
        "b": conv.bias.detach().double().numpy().tolist(),
        "inputs": conv.in_channels,
        "outputs": conv.out_channels,
    }


def toy_model(width=4):
    torch.manual_seed(1)
    net = Net(width)
    return {
        "name": "toy",
        "architecture": "dilated-cnn-2",
        "origin": "Synthetic",
        "observation_schema": "1",
        "fault_schema": "3",
        "spectrum_schema": "1",
        "shapes": SHAPES,
        "stem": conv_json(net.stem),
        "blocks": [conv_json(b) for b in net.blocks],
        "mix": conv_json(net.mix),
        "head": conv_json(net.head),
    }


def toy_data(n=6):
    rng = np.random.default_rng(2)
    mean = rng.integers(100, 200, (n, BINS)).astype(np.uint8)
    mean[0, 900:] = 255
    fall = np.zeros((n, BINS), np.uint8)
    fall[1:3, 100:300] = 40  # 10 dB
    bits = np.zeros((n, BINS), np.uint16)
    bits[1:3, 100:300] = 1 << SHAPES.index("LowMer")
    meta = []
    for i in range(n):
        shapes = [{"shape": "LowMer", "worst_fall_db": 10.0}] if i in (1, 2) else []
        meta.append({"map": "toy", "profile": "toy", "first_mhz": 811.0, "worst_fall_db": 10.0 if shapes else 0.0, "shapes": shapes, "causes": [], "service_group": "sg"})
    return {"min": mean.copy(), "mean": mean, "fall": fall, "bits": bits, "meta": meta}


class WeightEdit(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.parent = self.dir / "parent.json"
        self.parent.write_text(json.dumps(toy_model()))
        self.data = toy_data()

    def edit(self, ops, name):
        path = self.dir / f"{name}.edit.json"
        path.write_text(json.dumps({"ops": ops, "rationale": "a test"}))
        return path

    def test_the_toy_artifact_loads_into_the_network(self):
        model = we.load(self.parent)
        net = we.net_of(model, "cpu")
        self.assertEqual(we.weights(model), sum(p.numel() for p in net.parameters()))
        self.assertEqual(len(DILATIONS), len(model["blocks"]))
        self.assertEqual(model["stem"]["inputs"], INPUTS)

    def test_an_edit_round_trips(self):
        ops = [
            {"tensor": "head.w", "index": [we.FALL, None, None], "op": "scale", "value": 2.0},
            {"tensor": "head.b", "index": [we.FALL], "op": "add", "value": 0.5},
            {"tensor": "blocks.3.b", "index": [[0, 2]], "op": "set", "value": [1.0, -1.0]},
            {"tensor": "mix.w", "op": "outer", "u": [1.0, 0, 0, 0], "v": [0.1] * 8, "alpha": 2.0},
        ]
        out = we.apply(self.parent, self.edit(ops, "one"), self.dir / "child.json", device="cpu", data=self.data)
        before, after = we.tensors(we.load(self.parent)), we.tensors(we.load(out))
        np.testing.assert_allclose(after["head.w"][we.FALL], 2.0 * before["head.w"][we.FALL])
        np.testing.assert_allclose(after["head.w"][: we.FALL], before["head.w"][: we.FALL])
        self.assertAlmostEqual(after["head.b"][we.FALL], before["head.b"][we.FALL] + 0.5)
        np.testing.assert_allclose(after["blocks.3.b"][:2], [1.0, -1.0])
        np.testing.assert_allclose(after["mix.w"][0, :, 0], before["mix.w"][0, :, 0] + 0.2)
        np.testing.assert_allclose(after["mix.w"][1:], before["mix.w"][1:])
        # Everything `proposal::spectrum` reads is still there, unchanged in kind.
        child = we.load(out)
        for key in ("name", "architecture", "origin", "observation_schema", "fault_schema", "spectrum_schema", "shapes", "thresholds"):
            self.assertIn(key, child)
        self.assertEqual(child["edit_log"][0]["kind"], "inner")

    def test_the_log_grows_and_names_its_parent(self):
        head = [{"tensor": "head.b", "index": [we.FALL], "op": "add", "value": 0.1}]
        one = we.apply(self.parent, self.edit(head, "a"), self.dir / "one.json", device="cpu", data=self.data)
        two = we.apply(one, self.edit(head, "b"), self.dir / "two.json", device="cpu", data=self.data)
        log = we.load(two)["edit_log"]
        self.assertEqual(len(log), 2)
        self.assertEqual([e["kind"] for e in log], ["head", "head"])
        self.assertEqual(we.parent_of(two), one)
        self.assertEqual(we.load(two)["parent"]["sha256_16"], we.digest(one))
        # The parent is never written over.
        self.assertEqual(we.load(self.parent), toy_model())

    def test_the_test_maps_are_refused(self):
        with self.assertRaises(we.Refused):
            we.split_path("test")
        with self.assertRaises(we.Refused):
            we.read(we.load(self.parent), "test")

    def test_an_edit_with_no_reason_or_a_new_shape_is_refused(self):
        path = self.dir / "bare.json"
        path.write_text(json.dumps({"ops": [{"tensor": "head.b", "op": "add", "value": 1.0}]}))
        with self.assertRaises(we.Refused):
            we.apply(self.parent, path, device="cpu", data=self.data)
        with self.assertRaises(we.Refused):
            we.apply_ops(we.load(self.parent), [{"tensor": "mix.w", "op": "outer", "u": [1.0], "v": [1.0]}])
        with self.assertRaises(we.Refused):
            we.apply_ops(we.load(self.parent), [{"tensor": "nothing.w", "op": "add", "value": 1.0}])

    def test_the_reserve_maps_are_read_once(self):
        we.HELDOUT = self.dir / "heldout.json"
        we.HELDOUT.write_text("{}")
        with self.assertRaises(we.Refused):
            we.heldout(self.parent, self.parent, device="cpu")

    def test_parity_is_written_in_double_precision(self):
        out = we.parity(self.parent, data=self.data, cases=2)
        cases = json.loads(out.read_text())["cases"]
        self.assertEqual(len(cases), 2)
        self.assertEqual(len(cases[0]["outputs"]), len(SHAPES) + 1)


if __name__ == "__main__":
    unittest.main()
