# Real captures from public sources

Everything else under `fixtures/` is invented. These are the exception (invariant 11), and the only real device data in the repo.

| File | Source | License | What it is |
|---|---|---|---|
| `pypnm_rxmer.bin` | [PyPNM](https://github.com/PyPNMApps/PyPNM) `tests/files/rxmer.bin`, fetched 2026-11-04, SHA-256 `406d98c3…cc4b88` | Apache-2.0, © PyPNM's authors | A DOCSIS 3.1 RxMER per subcarrier file: 25 kHz, 7,480 subcarriers, 640–827 MHz |

Its provenance is only what the file shows. The MAC is the placeholder `a1:b2:c3:d4:e5:f6`, so it looks like a real capture with the address replaced, and the project doesn't say which modem or plant it came from. Treat it as one example, not as a measure of any network.

What it reads: mean 40.42 dB, standard deviation 1.13 dB, scatter past a 2 MHz average 0.79 dB, slope −1.18 dB per 100 MHz, and one spike to 28.25 dB at 750 MHz. `crates/pnm-wire/tests/field_capture.rs` holds those numbers, and the sim's healthy scatter comes from it.
