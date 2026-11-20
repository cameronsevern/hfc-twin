# HFC physics reference

The equations the sim implements, with worked examples that double as unit-test
oracles. Numbers marked **approx** are from memory or typical values and must be
replaced by datasheet values in the component library; the *equations* are not
approximate. Units: MHz, feet, dBmV (75 Ω), °C, unless stated.

Conventions: `dB` is a ratio, `dBmV` is an absolute level. `lin(x_db) = 10^(x_db/10)`
for power quantities. "Sum in linear" means `10·log10(Σ 10^(x_i/10))`.
A downstream level `L(f)` in dBmV is referenced to a 6 MHz bandwidth unless stated,
the same reference as the CM input window in §3.2. Upstream references differ; see
§7.2. Sources are cited by the short names in `docs/reference/README.md`.

---

## 1. Coaxial cable attenuation

### 1.1 Frequency dependence

Attenuation per 100 ft is dominated by conductor (skin-effect) loss ∝ √f and
dielectric loss ∝ f:

    A(f) = a·√f + b·f        [dB/100 ft, f in MHz]

Fit `a`, `b` per cable type from two or more datasheet points. Store `(a, b)` in
the component library, not the table.

**Typical datasheet points (approx, 20 °C):**

| Cable | 55 MHz | 550 MHz | 1000 MHz | 1218 MHz |
|---|---|---|---|---|
| .500 P3 hardline | 0.52 | 1.70 | 2.35 | 2.60 |
| .625 P3 hardline | 0.42 | 1.38 | 1.90 | 2.12 |
| .750 P3 hardline | 0.35 | 1.15 | 1.60 | 1.78 |
| RG6 drop | 1.5 | 4.8 | 6.5 | 7.2 |
| RG11 drop | 0.95 | 3.0 | 4.3 | 4.85 |

The drop rows sit just under the SCTE 74 ceilings (series 6: 6.56 at 1002 MHz and 7.26
at 1218 MHz; series 11: 0.96, 3.04, 4.36, 4.92).

**Worked example (test oracle):** .500 P3, a = 0.0598, b = 0.000460. These are
working coefficients, not an exact fit of the table: they match the 1000 MHz row
and are within about 0.05 dB of the others. (An exact fit of the 550 and 1000 MHz
rows is a = 0.0672, b = 0.000223, giving 2.01 dB/100 ft at 750 MHz.) At 750 MHz:
A = 0.0598·27.39 + 0.000460·750 = 1.638 + 0.345 = **1.98 dB/100 ft**. A 1,250 ft span → 24.8 dB.

RG6 drop (approx), fit from the 55 and 1000 MHz rows: a = 0.2013, b = 0.000136, so
A(750) = 5.61 and A(30) = 1.106 dB/100 ft.

### 1.2 Temperature dependence

Conductor resistance rises with temperature; attenuation scales approximately
linearly:

    A(f, T) = A(f, 20°C) · (1 + k_T · (T − 20))      k_T ≈ 0.0020 /°C  (approx; ≈ 1%/10°F)

T is `temp_c` for aerial spans and `ground_temp_c` for underground ones (§10), which
is why underground plant swings less.

**Oracle:** 24.8 dB at 20 °C → at 45 °C: 24.8 · 1.05 = **26.04 dB**; at −20 °C:
24.8 · 0.92 = **22.82 dB**. Annual swing on a long span is several dB, which is
why plant has AGC/thermal compensation (see §3.4).

### 1.3 Velocity of propagation

    v = v_f · c,   c = 983.6 ft/µs

v_f ≈ 0.87 for foam hardline, 0.85 for RG6/RG11 (approx). One-way delay over
length L ft: `τ = L / (v_f · 983.6)` µs.

**Oracle:** 200 ft of .500 P3: τ = 200 / (0.87·983.6) = **0.2337 µs**.

### 1.4 DC loop resistance (for powering, §6)

Loop resistance R_loop in Ω per 1000 ft (center + shield), approx:
.500 P3 ≈ 1.7, .625 ≈ 1.1, .750 ≈ 0.75. Datasheet values required.

---

## 2. Passives

### 2.1 Taps

A tap has a *tap value* (dB from input to each tap port) and an *insertion loss*
(through loss, input to output), both frequency-dependent and from the datasheet.
Model each as a small table indexed by frequency with linear interpolation in
dB, or as `(a, b)` fits like §1.1.

Typical (approx): insertion loss grows as tap value falls. Interpolate linearly
between these points and hold the value flat out to the component's rated band.
Outside the rated band a lookup is an error, not an extrapolation: legacy passives
can collapse above about 1.3 GHz [PNM-HFC §VIII.2.1.3.1].

| 4-port tap value | Insertion @ 55 MHz | Insertion @ 1000 MHz |
|---|---|---|
| 23 dB | 0.8 dB | 1.4 dB |
| 17 dB | 1.6 dB | 2.44 dB |
| 8 dB | 2.8 dB | 4.0 dB |

The last tap in a run either has no through port (a terminating tap, typically a
low value such as 4 dB) or has its through port capped by a terminator. A 4 dB tap
suits only a run long enough to bring the low-frequency level down: at 38 dBmV and
108 MHz, a modem on an 80 ft drop stays above +15 dBmV until roughly 2,600 ft of .500
from the active, less any tap insertion loss on the way.

Port isolation (tap-to-tap) ≥ 20 dB typical (approx). It affects upstream noise
funneling only at second order, but it sets the strength of off-path echoes (§5.1),
so each tap carries `port_isolation_db` and `return_loss_db`.

### 2.2 Splitters and directional couplers

Two-way splitter: 3.5–4.0 dB each leg (approx). Three-way balanced: 5.5–6.0 dB.
Directional coupler with coupled-port value X dB: through loss ≈
`−10·log10(1 − 10^(−X/10)) + 0.3` (the 0.3 is excess loss, approx).

**Oracle:** 8 dB DC: through = −10·log10(1 − 0.1585) + 0.3 = 0.75 + 0.3 =
**1.05 dB**.

---

## 3. Level budget (forward path)

### 3.1 Node/amp output and tilt

An active's output is specified at the top frequency f_hi with a *tilt*: level
rises linearly in dB with frequency from f_lo to f_hi so cable loss (∝ √f) is
partly pre-compensated.

    L_out(f) = L_out(f_hi) − tilt · (f_hi − f) / (f_hi − f_lo)

The profile sets these as targets, and the amp's spec only says what it can deliver.
Typical values (approx):

| Profile | L_out(f_hi) | Tilt | Over |
|---|---|---|---|
| Low-split I-CCAP | 50 dBmV | 14 dB | 54–1002 MHz |
| High-split RPD | 52 dBmV | 12.1 dB | 258–1218 MHz |

All code goes through `out_level(active, f)`, because
1.8 GHz plant uses stepped and cable-shaped output, not a single line (§11).

### 3.2 Level at any point

Walk the tree from the active. At each element subtract its loss at f:

    L(f, point) = L_out(f) − Σ cable A(f,T)·len/100 − Σ passive losses(f)

At a tap port: subtract tap value. At a modem: also subtract drop cable loss
and in-home splitting.

**Oracle (one-street fixture, high-split, 750 MHz, 20 °C):** amp output per §3.1:
52 − 12.1·(1218 − 750)/(1218 − 258) = 46.10 dBmV at 750. 400 ft .500 → −7.93 dB. 23 dB
tap insertion (§2.1) −1.24 dB. 300 ft .500 → −5.95 dB. 17 dB tap, take tap port:
−17.0 dB. 120 ft RG6 drop at 750 (§1.1) → −6.74 dB. Modem input = 46.10 − 7.93 − 1.24 −
5.95 − 17.0 − 6.74 = **7.24 dBmV**. On low-split, the amp output at 750 MHz is
46.28 dBmV, so the same modem reads **7.42 dBmV**. Both are inside the CM input window
(−15 to +15 dBmV per 6 MHz). This is the "§3.2 modem" in the oracles below.

**Oracle (whole fixture, −20 to 45 °C, amps holding the §3.1 targets):**

| Profile | Modem input (dBmV) | Largest spread at one modem | Highest total input |
|---|---|---|---|
| High-split RPD, 258–1218 MHz | **+5.08 to +13.97** | **4.84 dB** | about **34.0 dBmV** |
| Low-split I-CCAP, 54–1002 MHz | **+5.81 to +14.12** | **6.82 dB** | about **33.6 dBmV** |

Both meet every §3.3 limit, including the 4096-QAM floor.

### 3.3 Design window

A design is valid iff, for every modem and every f in the band it demodulates:

- `L(f) ≤ +15 dBmV` per 6 MHz, and `L(f)` is at or above the floor for the highest
  modulation in use [PHY Table 46]:

  | Highest QAM | 256 and below | 512, 1024 | 2048 | 4096 |
  |---|---|---|---|---|
  | Floor (dBmV per 6 MHz) | −15 | −12 | −9 | −6 |

- Average power per MHz is at most 10 dB above the average of the lowest-power 24 MHz
  the modem demodulates [PHY Table 45]. This is the spec's limit on tilt at the modem,
  and it is `psd_spread_max_db` in the profile.
- Total modem input power is below 40 dBmV [PHY Table 45].

The optimizer inserts an amp when the lowest-frequency or highest-frequency level
would leave the window before the next tap.

### 3.4 Unity gain

Each amp restores its output to the profile's L_out(f_hi) and tilt (unity gain
across a span+amp). Amp gain required = L_out(f_hi) − L_in(f_hi); it must be ≤
the amp's max gain and the input must be ≥ the amp's minimum input level
(approx 10–20 dBmV depending on the model). AGC holds this against temperature
(§1.2) within its range, typically ±3 dB (approx).

---

## 4. Noise, distortion, and MER

### 4.1 Thermal noise floor (75 Ω)

    N_floor = −125.2 dBmV/Hz + 10·log10(B_Hz)     (kT at 290 K; −174 dBm/Hz + 48.75 dB)

| Bandwidth | N_floor |
|---|---|
| 6 MHz (SC-QAM channel) | −57.4 dBmV |
| 25 kHz (subcarrier: downstream 8K FFT, upstream 4K FFT) | −81.2 dBmV |
| 50 kHz (subcarrier: downstream 4K FFT, upstream 2K FFT) | −78.2 dBmV |

**Oracle:** 25 kHz: −125.2 + 10·log10(25000) = −125.2 + 43.98 = **−81.22 dBmV**.

### 4.2 Carrier-to-noise of one active

    CNR_i = L_in,i − NF_i − N_floor(B)

where L_in is the input level at that frequency and NF the amp's noise figure
(approx 6–9 dB for a trunk amp; datasheet). L_in and N_floor must use the same
bandwidth: levels are referenced to 6 MHz, so use B = 6 MHz, or first scale L_in
by `10·log10(B / 6 MHz)`. Both give the same CNR for a flat signal. Pairing a
6 MHz level with a 25 kHz floor overstates CNR by 23.8 dB.

**Oracle:** L_in = 18 dBmV, NF = 7 dB, B = 6 MHz: CNR = 18 − 7 + 57.4 =
**68.4 dB**.

### 4.3 Cascade

Noise adds in linear power along the cascade (Friis, expressed in CNR form):

    1/lin(CNR_total) = Σ_i 1/lin(CNR_i)

For N identical stages: `CNR_N = CNR_1 − 10·log10(N)`.

**Oracle:** six amps each 68.4 dB → 68.4 − 7.78 = **60.62 dB**.

### 4.4 Composite distortion

Legacy analog-style metrics still apply to amp behavior:

- CTB (composite triple beat) degrades **2 dB per 1 dB** of output level increase
  and cascades at **20·log10(N)**.
- CSO (composite second order) degrades **1 dB per 1 dB** and cascades at
  **15·log10(N)** (empirical).
- CIN (composite intermodulation noise, the noise-like distortion relevant to
  QAM/OFDM loading) cascades at **10·log10(N)** and is treated as additional
  noise.

Datasheet gives CTB/CSO/CIN at a reference output level and channel loading.
Scale to the actual output:

    CTB(L) = CTB_ref − 2·(L − L_ref),   CSO(L) = CSO_ref − 1·(L − L_ref),   CIN(L) = CIN_ref − 1·(L − L_ref)

These slopes and cascade factors are approx: no source in `docs/reference` states them.
CIN is third-order dominated, so its true slope may be nearer 2 dB per dB. Above an
amp's `compression_onset_dbmv` (§8), CIN degrades a further 2 dB per dB (approx).

### 4.5 MER at the modem

Combine all noise-like impairments in linear power:

    1/lin(MER) = 1/lin(MER_src) + 1/lin(CNR_opt) + Σ_i 1/lin(CNR_i) + 1/lin(CIN) + 1/lin(CNR_cm) + 1/lin(MER_impl) + 1/lin(MER_isi) + 1/lin(MER_ingress)

**Reflections have no noise term of their own.** The modem's equalizer undoes the
channel, so a noise source's contribution depends on the signal at the point where
that noise enters [PNM-HFC §5.3.4.2]. Noise added before a reflection is shaped along
with the signal and stays flat in RxMER. Noise added after it is not, so RxMER ripples
as |H(f)|². Compute every CNR term from the signal at that source's own location,
echoes included. Then ripple appears on its own, without a scripted penalty.

- `MER_src`: the CMTS or RPD output. PHY Table 41 requires a channel average of at
  least 50 dB below 600 MHz, 47 dB from 600 to 1002 MHz and 45 dB from 1002 to 1218 MHz.
  Use those floors on both node types (approx; real devices may do better).
- `CNR_i`: each active in the cascade (§4.2, §4.3).
- `CNR_cm(f) = L(f) − NF_cm − N_floor(6 MHz)`: the modem's own noise, after every plant
  echo, which is what makes low level and tilt lower and shape RxMER. `NF_cm` is about
  10 dB (approx): PHY Table 46 requires 41 dB at −6 dBmV.
- `MER_impl`: the modem's implementation ceiling (approx 43–45 dB for D3.1 modems; one lab
  modem reads about 47 dB). It enters after the plant's echoes, so part of it ripples.
  `1/lin(MER_impl)` becomes `(s/|H(f)|² + (1 − s))/lin(MER_impl)`, with `s` the share made
  in the modem's front end and quantizers rather than by multiplicative effects such as phase
  noise [HRANAC2019 §3.1.1]. `s = 0.65` is fitted to one modem, so approx.
  **Oracle** [HRANAC2019 Test Cases 2B, 2C]: channel ripple 6.97 dB p-p is ρ = 0.381 (§5.2).
  With `s = 0.65` and `MER_impl` = 46.79 dB and no other noise: mean **46.65 dB**, standard
  deviation **1.60 dB**, ripple **4.72 dB** p-p. The lab read 46.65 and 46.66 dB, 1.57 and
  1.62 dB, at 0 and +10 dBmV. It didn't change with level, which a fixed `NF_cm` can't do: at
  high input a modem's front end attenuates and its noise figure rises [HRANAC2019 §3.1]. We
  don't model that, so the sim's ripple grows slightly with level.
- `CNR_opt`: the forward analog optical link. Analog nodes only; an RPD has digital
  optics and omits the term. Use 48 dB (approx, our estimate: no source gives a
  figure), so any RxMER gain from migrating is a what-if until real data calibrates it.
  A hub-shelf RPD with linear optics [R-PHY §5.1] is out of scope for v1.
- `MER_isi`: echoes longer than the cyclic prefix add noise of about
  `2·(τ − T_CP)·ρ²/T_U` relative to the signal, ISI plus about as much ICI [PHY §7.4.10].
  SC-QAM has no cyclic prefix; its demodulator's adaptive equalizer takes an echo out, as far as its span
  reaches [Hranac, "How Adaptive Equalization Works"] (task 27). An echo of level `E` and delay `τ` leaves a
  residual `E − C` for `τ ≤ τ_span` and `E` beyond, with `C = 30 dB` and `τ_span = 1.5 µs` (approx, both
  [inferred]: DOCSIS assumes a modem works through a dominant echo of −10 dBc to 0.5 µs, −15 to 1.0, −20 to
  1.5 and −30 beyond [DOCSIS 3.0 PHY, unverified] (impairments.md §2: read from secondary summaries, not the
  specification), so the span ends where the assumption drops to −30, and `C` leaves its worst echo 40 dB down). `MER_isi = −(E − C)`, of the largest residual only: the
  equalizer takes out the rest. Summing over every small echo on a dense plant would put dozens at −40 dB
  at −26 dB in total, and take healthy D3.0 modems offline, which plant doesn't do. Until task 27 there was
  no `C`, and an open cut's echo through the isolation of the tap above it took that tap's 3.0 modems off.

  **Oracles (task 27).** −15 dBc at 0.29 µs: residual −45 dBc, **MER_isi 45 dB**. −10 dBc at 0.5 µs:
  **40 dB**. −15 dBc at 2.0 µs, past the span: **15 dB**.
- `MER_ingress`: applies only to subcarriers inside a downstream ingress band (§8):

      MER_ingress(f) = L(f) − (P_ingress + 10·log10(6 / bw_mhz) − Loss(entry → modem, f))

  with `P_ingress` the total ingress power in `bw_mhz`, so both terms are per 6 MHz.

Per-subcarrier: evaluate every term at that subcarrier's frequency. This is what
produces RxMER-per-subcarrier that tilts, dips and ripples the way real PNM
data does.

**Oracle (the §3.2 modem, high-split RPD, 750 MHz):** MER_src = 47. The one amp in its
cascade has 46.10 − 23.79 = 22.31 dBmV at its input, so with NF = 7, CNR = 22.31 − 7 +
57.4 = 72.71. CIN = 58, CNR_cm = 7.24 + 57.4 − 10 = 54.64, MER_impl = 44, no ingress or
far echo: lin sum = 2.00e−5 + 5.4e−8 + 1.585e−6 + 3.44e−6 + 3.98e−5 = 6.48e−5 →
MER = **41.88 dB**. (Source and modem ceilings dominate on clean plant, and the guide's
healthy example averages 41.2 dB [PNM-HFC §5.3.4.3].) Adding CNR_opt = 48 dB, as a
what-if on an analog node, gives **40.93 dB**, a gain of **0.95 dB**.

**Oracle (downstream ingress):** the same modem, with −30 dBmV of ingress in 10 MHz
entering at the modem (no path loss). Per 6 MHz: −30 + 10·log10(6/10) = −32.22 dBmV,
so MER_ingress = 7.24 + 32.22 = **39.46 dB**. Combined with the clean 41.88 dB:
**37.49 dB**.

### 4.5.1 An adjacent channel's leakage into OFDM

A modem's FFT also collects energy from whatever sits beside the OFDM channel, since an SC-QAM isn't
on its subcarrier grid. Taper-region overlap and leakage depend on Ncp and Nrp, and raising both reduces it [HRANAC2019 §3.2].
It shows on modems whose window uses the CP's spare time, which we model as `T_w = T_CP − T_rp`. With
`β = T_w/T_U` and `k_c = 1/(2β)`, the power leakage at `k` subcarriers from a source, with sin² averaged to ½, is
about `1/(2π²k²)` for `k < k_c` and that times `½(k_c/k)⁴` beyond it (the windowed sinc envelope). Summed over an
adjacent channel `x` subcarriers away, taken as wide compared to `k_c`:

    I(x) = (1/x − 1/k_c)/(2π²) + 1/(20π²·k_c)       for x < k_c
    I(x) = k_c⁴/(20π²·x⁵)                           for x ≥ k_c
    1/lin(MER_adj)(x) = lin(L_adj − L_ofdm)·I(x)

`L_adj` and `L_ofdm` are levels per 6 MHz at the modem, so a high block of SC-QAMs costs more on its own. Approx:
the form is ours, with the shape and direction checked against field and lab readings only. Some modems show this and others
on the same signal don't [HRANAC2019 §3.2]; we don't model that spread.

**Oracle:** 50 kHz (T_U = 20 µs), Ncp 192 and Nrp 128 (0.9375 and 0.625 µs), so T_w = 0.3125 µs, β = 0.015625,
k_c = 32. An SC-QAM edge 1 MHz off, x = 20, equal levels: I = (0.05 − 0.03125)/19.739 + 1/(197.39·32) = 9.499e−4 +
1.583e−4 = 1.108e−3, **29.55 dB**. At 2 MHz, x = 40: 32⁴/(197.39·40⁵) = 5.19e−5, **42.85 dB**. With Ncp 1024 and Nrp
256 (5 and 1.25 µs), k_c = 2.667, at x = 20: 50.57/(197.39·20⁵) = 8.0e−8, **71.0 dB**, gone.

### 4.6 MER thresholds and codeword errors

Two different quantities, which must not be mixed:

| QAM | 16 | 64 | 128 | 256 | 512 | 1024 | 2048 | 4096 |
|---|---|---|---|---|---|---|---|---|
| Failure threshold, `thr` (dB) | 12* | 18* | 21* | 24 | 26 | 29 | 32 | 35 |
| Required CNR (dB) | 15 | 21 | 24 | 27 | 30.5 | 34 | 37† | 41† |
| Recommended average RxMER (dB) | | | | 29–30 | 31–33 | 34–36 | 37–39 | 40–42 |

`thr` is where uncorrectable codewords begin [PNM-HFC Table 6, "fail" column], and it
drives the curves below. Required CNR is what a modem must meet in AWGN at 1e−6 packet
error ratio [PHY Table 46], and it is the default required MER behind a reported SNR
margin [CM-OSSI Table 72]. Recommended RxMER is good operating practice [PNM-HFC
Table 5].

\* approx: not in the guide; extrapolated at 3 dB per modulation order.
† 37.5 and 41.5 dB from 1002 to 1218 MHz.

**Effective margin (our model, approx).** The sources give no mapping from non-flat
RxMER to FEC. LDPC and interleaving spread a codeword across subcarriers, so a narrow
dip is survivable where a 2nd-percentile test would call it fatal. Average mutual
information over the profile's data subcarriers, capped so that excess margin on good
subcarriers can't buy back bad ones:

    MI_k       = min( log2(1 + lin(RxMER_k)), log2(1 + lin(thr + 1.25)) )
    RxMER_eff  = 10·log10( 2^(mean_k MI_k) − 1 )
    m          = RxMER_eff − thr

The 1.25 dB cap is fitted to the guide's notch tests [PNM-HFC §V.3.2]. On flat RxMER,
`m` is simply `RxMER − thr`, up to the cap.

**OFDM uncorrectables** fall about seven decades in about 1 dB (approx, read from
[PNM-HFC Fig. 74]), with `k` = 15 at 256-QAM and below, 10 at 4096-QAM, and linear in
bits per symbol between:

    P_uncorr,OFDM(m) = 1 / (1 + exp(k·(m + 0.55)))

**OFDM correctables** count codewords with errors *before* decoding, across a
16,200-bit LDPC block, so they are common on healthy links and are not a fault
indicator, unlike SC-QAM [PNM-HFC §5.4.5]. They follow the raw margin, `m_raw =
mean(RxMER) − thr`, not the capped one. Measured only at 256-QAM [PNM-HFC Fig. 76], so
other orders are approx:

    P_err,OFDM(m_raw)  = 1 / (1 + exp(2.2·(m_raw − 6.8)))
    P_corr,OFDM        = max(P_err,OFDM − P_uncorr,OFDM, 0)

**SC-QAM** (Reed-Solomon, counted after the inner decoder) has a much shallower slope.
Its 256-QAM threshold is about 26 dB (approx, [PNM-HFC Fig. 74]); source other orders
when needed. Margin is against channel MER, and the correctable rate uses the same
shape shifted +1.3 dB. Model (approx, v1):

    P_uncorr,SC(m) = 1 / (1 + exp(5·(m − 0.2)))

Codeword counts per interval = rate × codewords/s (from the channel plan). Replace
these curves with measured ones when available.

**Oracle (OFDM 4096-QAM, flat RxMER):** at the §4.5 clean-plant 41.88 dB, m = 1.25
(capped) gives P_uncorr = **1.5e−8**, and m_raw = 6.88 gives P_corr = **0.455**.

**Oracle (notch, 4096-QAM, k = 10):** an 8 MHz band of low RxMER with the rest at
41.88 dB. In 190 MHz of active spectrum, RxMER of 9 dB in the band gives m = 0.12 and
P_uncorr = **1.2e−3**. In 94 MHz, RxMER of 15 dB gives m = −0.55 and P_uncorr = **0.49**.
Those values are RxMER in the band, not notch depth. The guide's figures show
uncorrectables near 8–9 dB and near 15 dB respectively, ±1–2 dB [PNM-HFC §V.3.2].

FEC is counted per OFDM channel and per profile, so each profile uses its own
modulation's threshold from the channel plan. The PLC is always 16-QAM [PHY §7.5.13.2]
and the NCP is QPSK, 16-QAM or 64-QAM. No source says where they fail, so approximate
them with the data curve at their modulation. They report only failures and totals.
Guidance is to see no unreliable PLC codewords, and PLC correctables can be ignored
[PNM-HFC §5.10.3].

---

## 5. Reflections and pre-equalization

### 5.1 Echo from a cavity

An echo needs two reflectors. A cavity of d ft of cable between reflectors with return
losses RL_1 and RL_2 produces [PNM-HFC §6.2.1.1]:

    delay   τ = 2·d / (v_f · 983.6)  µs
    level   E = −(RL_1 + RL_2 + 2·A(f)·d/100)  dB relative to main signal

τ is the **spacing between the reflectors**, not the distance to the modem. The modem
or its tap port is only one possible near reflector (approx 16–20 dB). A fault pairs
with the nearest other mismatch in the plant graph, which comes from the
`return_loss_db` on taps, amps and modems.

- **A cavity in the plant** gives the same τ to every modem below it.
- **A cavity ending at a modem** gives a τ that differs by drop.
- **An off-path reflector** lies beyond the modem's tap, and the signal reaches it by
  leaking through tap isolation. It is one echo, about `isolation − tap value` below
  main before the reflection loss: a 23 dB tap at 38 dB isolation leaks 15 dB down, so
  it shows most on high-value taps [PNM-HFC §6.2.1.1.2]. It reaches modems whose path
  never crosses the fault.

**Oracle:** d = 150 ft .500 (v_f 0.87), RL_1 = 10 dB (corroded connector),
RL_2 = 18 dB, A(750) = 1.98 dB/100 ft: τ = 300/855.7 = **0.3506 µs**;
E = −(10 + 18 + 5.94) = **−33.9 dB**.

### 5.2 Effect on the frequency response

An echo of linear amplitude ρ = 10^(E/20) at delay τ produces amplitude ripple
with period 1/τ in frequency:

    |H(f)| = |1 + ρ·e^(−j2πfτ)|,   peak-to-peak ripple ≈ 20·log10((1+ρ)/(1−ρ)) dB

**Oracle:** E = −33.9 dB → ρ = 0.0202 → ripple p-p = 20·log10(1.0202/0.9798)
= **0.35 dB**, period = 1/0.3506 µs = **2.85 MHz**. Group delay ripple has the
same period. At E = −10 dB and 500 ns: **5.69 dB** p-p, **2 MHz** period.

Equalization removes this from RxMER except as §4.5 describes, which is why channel
estimate and pre-eq coefficients, not MER, are the sensitive indicators.

### 5.3 Upstream pre-equalization (DOCSIS 3.0 SC-QAM)

24 taps at symbol spacing T = 1/R_sym (R_sym = 5.12 Msym/s → T = 0.1953 µs), main tap
usually at position 8 (1-based; report it as `main_tap_index`, because it can differ).
Compute the taps as the T-spaced inverse of H(f) over `fc ± R_sym/2`, with H folded through
the matched filter: a T-spaced equalizer sees `Σ_m |P(f + m/T)|²·H(f + m/T)`, raised cosine with
α = 0.25, so in the roll-off two images of the echo partly cancel unless τ/T is whole (our
reading, not stated by a source; it brings §12's reflection row closer). An echo between
tap positions spreads across neighbors, around `8 + τ/T` [PNM-HFC §6.2.3.4]. Reported
metrics [PNMP §6.3.6–6.3.8], with MTE the main tap's energy, NME the energy in all
other taps and TTE = MTE + NME:

    MTC   (main tap compression)            = 10·log10( TTE / MTE )   [dB]
    MTR   (main tap ratio)                  = 10·log10( MTE / NME )   [dB]
    NMTER (non-main tap to total energy)    = 10·log10( NME / TTE )   [dB]

NMTER ≈ −MTR except at extreme distortion. MTC is near 0 dB on healthy plant.

**Oracle:** τ = 0.3506 µs → τ/T = 1.795, so energy peaks at tap **10**, with some at
tap 9. This is an upstream echo, so §5.1 is evaluated at the upstream frequency. At
30 MHz, A = 0.0598·5.477 + 0.000460·30 = 0.341 dB/100 ft, so E = −(10 + 18 +
2·0.341·1.5) = **−29.02 dB** and ρ = 0.0354. Without folding, total energy wouldn't depend on
the spread and MTR would be +29.0 dB. τ/T = 1.795 loses a little in the roll-off, so MTR ≈
**+29.3 dB**, NMTER ≈ **−29.3 dB** and MTC ≈ **0.005 dB**.

**Oracle (shape, with tolerance):** −10 dBc at 0.5 µs gives τ/T = 2.56. The guide's
field string peaks at taps 10 and 11 (−17.1 and −14.4 dB), with taps 9 and 12 at −25.5
and −24.3 dB [PNM-HFC Table 22].

Severity thresholds belong in diagnoser config, not the sim, and several schemes exist
[PNM-HFC §8.11]: the −29.0 dB echo above is in the monitor band by NMTER (−30 and −27 dB
[PNMP §6.3.8]) but passes MTR schemes. They apply to SC-QAM only; no thresholds exist
for OFDMA.

### 5.4 OFDMA pre-equalization (DOCSIS 3.1)

Frequency-domain: one complex coefficient per subcarrier, `c_k ∝ 1/H(f_k)`. The CM
normalizes them so that `mean(|c_k|²) = 1`, and I and Q each clip at the s2.13 limit,
just under ±4 [PHY §7.4.17]. Flat loss therefore goes into transmit power (§7.2), not
the coefficients, and `mag_mean_db` sits near 0 dB. Excluded subcarriers are zero.

An echo appears as ripple in |c(f)| with period 1/τ. Estimate τ by FFT of the
coefficient magnitude across subcarriers, and convert to cavity length with §5.1, which
is the guide's `D_ft = 492·v_f / ripple_spacing_mhz` (2.85 MHz at v_f 0.87 → 150 ft).
This is the localization primitive, used with topology: modems on one tap sharing τ
points to a cavity above the tap, and differing τ points to their drops. It is not
conclusive alone, since one fault can show different delays at different modems
[PNM-HFC §8.10.3].

### 5.5 Coefficients and summary metrics (DOCSIS 3.1)

The downstream channel estimate is the path response H(f) per subcarrier, scaled to an
average power of about 1 [CM-OSSI §D.2.6], so slope and ripple survive and §3.2's flat
loss does not. OFDMA pre-eq coefficients are the upstream inverse, normalized the same
way (§5.4). Both report the same summary metrics [PNM-HFC §5.5.7, §6.2.2].

**RxMER summary.** Over the active subcarriers' dB values: the mean, the population
standard deviation (divide by N), and the 2nd-percentile value, at 1-based sorted index
`floor(N·0.02)`. Guidance is a standard deviation below 2 dB, and 1 dB or less is better.

**Oracle:** 97 subcarriers at 40 dB and 3 at 30 dB: mean **39.7 dB**, standard
deviation **1.71 dB**, 2nd percentile **30 dB**.

**Slope and ripple.** Fit a least-squares line to y against subcarrier frequency in
MHz, with y the magnitude in dB, `10·log10(I² + Q²)`, or the group delay in ns. The
slope is the line's slope. With residuals `r = y − fit`, RMS ripple is
`√(Σr²/N)` and peak-to-peak ripple is `max r − min r`.

**Oracle:** x = 0, 1, 2, 3 and y = 0, 2, 0, 2: slope **0.4**, RMS ripple **0.894**,
peak-to-peak ripple **2.4**.

**Group delay.** From the unwrapped phase φ, with Δf the subcarrier spacing in Hz:

    τ_i = −(φ_i − φ_(i−1)) / (2π·Δf) · 10⁹   [ns]

This yields N − 1 values.

**Oracle:** a pure 0.3506 µs delay at 25 kHz spacing steps φ by −0.05507 rad per
subcarrier, which gives τ = **350.6 ns**.

### 5.6 Band-edge group delay

Diplex filters and AC chokes add group delay near their band edges, with no reflection
involved. It rises with cascade, up to about 300 ns in a band-edge channel [PNM-HFC
§6.2.6.3.12], and one field example reads −15.9 dB NMTER at N+5 [PNM-HFC Table 12]. It
is a property of healthy plant, not a fault, or the diagnoser would mis-flag real
modems. Each split an active supports carries a `group_delay_ns` table of frequency and
ns (`Generic`), because the band edges move with the diplexer, and it adds to the phase
of H(f) along the path.

---

## 6. AC powering

### 6.1 Load model

Each active draws roughly constant **power** P_i (datasheet, approx 40–90 W for
a trunk amp, 100–250 W for a node) from the 60/90 VAC quasi-square-wave line.
Current at the device: I_i = P_i / V_i. Because V_i depends on the drop, solve
iteratively (2–3 passes converge).

### 6.2 Voltage drop along the tree

For each cable segment carrying downstream current I_seg (sum of all active
currents fed through it):

    ΔV = I_seg · R_loop · len / 1000

Walk from the power supply outward; V at a device = V_supply − Σ ΔV on its path.
Power inserters and power-passing taps add negligible resistance; non-power-
passing devices block current (model as a cut).

**Oracle:** supply 90 V, one segment 1,500 ft .500 (R_loop 1.7 Ω/kft) feeding
two amps of 60 W each. Pass 1: assume 90 V → I = 2·60/90 = 1.333 A → ΔV =
1.333·1.7·1.5 = 3.40 V → V = 86.6. Pass 2: I = 120/86.6 = 1.386 A → ΔV = 3.53
→ V = **86.47 V**. Pass 3 changes by < 0.01 V.

### 6.3 Constraints

- Every active: V_in ≥ V_min (approx 40–45 V for 60 V plant, 60–65 V for 90 V).
- Supply: Σ I ≤ 0.8 × I_rated [SCTE 183 §16.12] (I_rated of 15 A is approx).
- A power-supply fault (§8) is modeled as reduced V_supply; brownout occurs
  when an amp's V_in < V_min, which the sim reports as the amp dropping out. Modems
  downstream lose the signal and fall through §9's channel and registration states,
  intermittently if V hovers near V_min. No source confirms this signature, so it is
  our own inference.

---

## 7. Return path

### 7.1 Upstream path loss

Same walk as §3.2 but at upstream frequencies (5–85 / 5–204 MHz) toward the
node, with tap value counted at the modem's tap port and amp return gain
restoring unity at each active.

### 7.2 Modem transmit power

    P_tx = P_in,node + Loss_up(f)

`P_in,node` is the level the node's return input needs, per 6.4 MHz, so this P_tx is
per 6.4 MHz too. The CMTS upstream port is set to 0 dBmV per 6.4 MHz through 1024-QAM,
7 dBmV for 2048-QAM and 10 dBmV for 4096-QAM [PHY Table 17]. The return path between
the node and the CMTS sets how much higher `P_in,node` must be, and a profile holds it:

- **Low-split analog node:** 8 dBmV per 6.4 MHz (approx). Fully loaded on 37 MHz that
  is 15.6 dBmV total. SCTE 183's worked example comes to 8.4 dBmV at the laser input.
- **High-split RPD:** the set point plus RFM gain. Use 10 dBmV, the bottom of the
  4096-QAM set point range [R-PHY §E.2], or 24.9 dBmV total on 199 MHz.

D3.1 modems report OFDMA transmit power per 1.6 MHz, which reads 6.02 dB lower
(10·log10(6.4/1.6)), within 17 to 53.2 dBmV. SC-QAM total power is a separate value
(§9).

**Transmit limit.** `Pmax` is the only modem input: at least 65 dBmV for D3.1 (vendors
may go higher), and a total composite power of 57 dBmV for D3.0 with at least 32-QAM
[PNM-HFC §8.14.1]. For D3.1 the rest is derived from the channels in the Transmit Channel Set
[PHY §7.4.12.2]:

    Neq    = ceil(BW_legacy / 1.6) + ceil(BW_OFDMA / 1.6)
    P1.6hi = Pmax − 10·log10(Neq)

capped at `53.2 + (Pmax − 65)` when the modulated spectrum is 24 MHz or less. Compare
the required power per 1.6 MHz against P1.6hi. Channels in a set must also stay within
the 12 dB dynamic range window.

**Oracle:** Pmax = 65: 80 MHz → Neq = 50 → **48.01 dBmV**; 96 MHz → **47.22 dBmV**;
192 MHz → **44.21 dBmV**.

When the required power exceeds the limit, the modem can't deliver it, so the CMTS
hears it low and SNR falls by the deficit. FEC, profile downgrades and T3 timeouts
follow from that (§7.3, §4.6). Don't test the limit and set `partial_service`: that
scripts a symptom (invariant 5), and the spec doesn't tie partial service to the limit
[MULPI §8.4, §10.3]. How far a modem saturates is vendor policy, so it is a profile
parameter that clamps by default (approx).

**Oracle (the §3.2 modem, 30 MHz, 20 °C, low-split):** `P_in,node` = 8 dBmV, amp
return gain restoring unity back to the node (§7.1). Cable: 700 ft .500 at
0.341 dB/100 ft → 2.39 dB. 23 dB tap insertion → 0.8 dB (§2.1). 17 dB tap port →
17.0 dB. 120 ft RG6 at 1.106 dB/100 ft → 1.33 dB. Loss = 21.52 dB, so P_tx =
**29.52 dBmV** per 6.4 MHz, reported as **23.50 dBmV/1.6 MHz**.

**Oracle (same modem, 204 MHz, high-split):** `P_in,node` = 10 dBmV. Cable 6.64 dB,
insertion 0.89 dB, tap port 17.0 dB, drop 3.48 dB. Loss = 28.01 dB, so P_tx =
**38.01 dBmV** per 6.4 MHz, reported as **31.99 dBmV/1.6 MHz**. That rise in loss with
frequency is what high-split costs in transmit headroom.

**Oracle (whole fixture, 20 °C):** the test asserts every modem on the healthy fixture
is at or above the reportable 17 dBmV/1.6 MHz and at or below P1.6hi:

| Profile | Across its band | Reported (dBmV/1.6 MHz) | P1.6hi |
|---|---|---|---|
| Low-split, 5–42 MHz | 37 MHz | **21.09 to 28.57** | 51.20 |
| High-split, 5–204 MHz | 199 MHz | **23.09 to 35.13** | 44.03 |

### 7.3 Noise funneling

All upstream legs on a node's RF port combine; noise powers add in linear. Funneling is
per port, since a node may keep ports separate for some channel types [R-PHY §5.4.3]:

    N_node = N_coax + Σ_ingress lin(N_ingress)

With unity return gain, every amp in the node adds its own thermal noise at the node
input, whatever its cascade depth, where M is the total amps in the node [SCTE 183
§16.5]:

    N_coax(B) = N_floor(B) + NF_ret + 10·log10(M)

Ingress at one home enters at that home's path loss to the node. Upstream SNR at the
CMTS = P_rx − N_node(B), combined in linear with `us_optics_snr_db` on an analog node
(40 dB, approx, our estimate) and nothing on an RPD. This is why one bad drop can degrade
a whole service group's upstream.

**Oracle (healthy fixture, M = 2, NF_ret = 10 dB, 6.4 MHz):** N_coax = −57.14 + 10 + 3.01
= **−44.13 dBmV**. Low-split: SNR = 8 + 44.13 = 52.13 dB, and with 40 dB optics
**39.74 dB**. High-split RPD: 10 + 44.13 = **54.13 dB**.

Modem data generally doesn't localize upstream ingress. The method is to attenuate or
disconnect legs one at a time and watch the node's upstream spectrum capture, plus
upstream data analysis on modems that support it [PNM-HFC §8.8.2]. Per-modem
transmit-power deltas help discover topology instead. So `N_node(f)` responds to each
active's `return_atten_db`, the input a technician's switch test changes.

Upstream OFDMA RxMER per subcarrier (measured at the CMTS, [PNM-HFC §6.3]) is the same
ratio at each subcarrier's frequency: `us_rxmer(f) = P_rx(f) − N_node(f)`, with both
referenced to the same bandwidth.

### 7.4 Modem state

A modem carries state between time steps, so the sim is a fold over the environment
series: still deterministic, and each step's length comes from the sample spacing. The
timers and rules below are normative [MULPI Annex B, §10.3]. The rates are our own model
(see `docs/reference/mac-operations.md`), since the spec doesn't give them.

**Upstream.** Each station-maintenance opportunity can fail, with probability `p_loss`
from the channel's burst SNR (§7.3) and from the downstream carrying the reply (§4.6).
`ranging_spacing_s` is a profile parameter because it's CMTS-dependent.

- T3 expiries per step are drawn from a seeded binomial with `floor(step / spacing)`
  trials, and can't exceed one per 200 ms per channel.
- 17 consecutive misses exhaust retries and make the channel unusable. One reply resets
  the count, so independent loss exhausts at `p_loss^17` per attempt sequence.
- T4 (30 s by default) expires only when no opportunity reaches the modem, which needs a
  downstream fault. An upstream-only fault raises T3 and leaves T4 flat.

**Downstream channels.** A profile failing while others work is partial channel. All of
a channel's profiles failing, or its pilots and preambles lost, makes that channel
unusable, which is downstream partial service [MULPI §8.4, §10.6.3]. Neither state takes
the modem down by itself: with FEC failing but pilots held, a primary channel with no
backup keeps being used [MULPI §10.6.1.2, cases 3–5].

A channel's RxMER is measured on pilots and preambles, whose values are known [PHY §9.3.6].
So a channel whose profiles have all failed still has a curve, and only a channel with
its pilots gone has none (`PILOT_LOSS_DB`, approx) [inferred].

**What holds the modem up** is whether MAC messages decode. On each channel with a signal
and pilots, the chance of losing one is:

- SC-QAM: `p_uncorr` (§4.6).
- OFDM: `1 − (1 − p_A)(1 − p_NCP)`, where `p_A` is `p_uncorr` on the channel's most robust
  profile. Without the NCP no codeword can be found [PNM-HFC §5.10]. That management
  traffic rides that profile is [inferred] from Profile A being what every modem on the
  channel decodes [MULPI §10.4].

`p_down` is the smallest of these, since the modem moves to whichever channel still works
[MULPI §10.6.1], and 1 if no channel qualifies. Each ranging opportunity is carried by a
MAP, so per channel and opportunity:

- With probability `p_down` the MAP is lost: nothing is sent, no T3 starts, and T4 keeps
  running. T4 expires after 30 s without an opportunity [MULPI §10.3]. At a spacing of 15 s,
  that's 2 consecutive lost MAPs.
- Otherwise the exchange runs, and can fail with `p_loss` as above.

A T4 expiry or exhausted retries make that upstream channel unusable until its next reply.

**Transitions.** An unusable upstream channel with another still usable is partial service.
If none remains, the modem reinitializes: `reg_state` falls. While `p_down` is 1 it stays
down. Otherwise it spends `reregistration_s` (profile parameter, approx) coming back, then
one registration attempt fails with probability `p_down⁴`: the first REG-REQ and T6's 3
retries, independently [inferred; MULPI Annex B]. A failed attempt starts over (`T6_EXPIRED`).

So a modem near the edge goes up and down: this is the intermittent downtime field staff
describe, and it isn't scripted. It's narrow on a steady fault, since LDPC's waterfall is:
**where a modem drops depends on its boot profile**, set by the operator. A fault that
varies in time widens it. The fixtures boot on 256-QAM (`generic`), and a boot profile can be
lower, such as 64-QAM or 16-QAM, for the worst parts of the plant [VIAVI2016]. No source gives a share.

**Oracles:** at p_loss = 0.5, exhaustion per sequence is **7.6e−6**. A 300 s step caps at
**1,500** T3 expiries per channel. Total downstream loss reinitializes the modem at about
**30 s**.

Flat RxMER on a 256-QAM boot profile (`thr` 24 dB, k 15, so `p = 1/(1 + e^(15(m + 0.55)))`),
with the fall in T4 and T6 at 15 s spacing:

| RxMER, dB | m | `p_A` | 2 lost MAPs, `p²` | Attempt fails, `p⁴` | Expect |
|---|---|---|---|---|---|
| 24.0 | 0 | **2.6e−4** | 6.8e−8 | 4.6e−15 | Stays up |
| 23.5 | −0.5 | **0.3208** | 0.1029 | 0.0106 | Up and down |
| 23.0 | −1 | **0.99883** | 0.9977 | 0.9953 | Mostly down |

The same 23 dB on a 64-QAM boot profile (`thr` 18 dB, m = 5) gives `p_A` = e^(−83.25), about
**7.0e−37**, so the modem stays registered with its 256-QAM profile and above failed.

---

## 8. Fault parameterizations

Each fault is a set of physical parameters that modifies the plant graph before
the sim runs. No fault touches telemetry directly. A `location` is a point on a span
(`span_id` + `pos_ft`) or a device port (`element_id` + `port`). A `schedule` holds 24
hourly multipliers on the fault's severity, with hour = (t / 3600) mod 24 and t = 0 at
midnight on the simulated day.

| Fault | Parameters | Plant modification |
|---|---|---|
| Water ingress (span) | `span_id`, `pos_ft`, `affected_len_ft`, `k_w` (dB/100ft at 1 GHz), `rain_coupling`, `impedance_variation_pct` | Adds `k_w,eff·√(f/1000)` dB/100 ft over `affected_len_ft` starting at `pos_ft`, with `k_w,eff = k_w·(1 + rain_coupling·rain_mm_per_hr)` (approx; §10). Seeded per-segment impedance variation over the same length produces the non-periodic waves that identify water [PNM-HFC §7.1.7.3] |
| Connector / reflection | `location`, `return_loss_db` | Adds a mismatch; cavities with the nearest other mismatches follow from §5.1, including off-path modems |
| Suckout / notch | `location`, `f0_mhz`, `depth_db`, `width_mhz` | Adds `depth·exp(−((f−f0)/width)²)` dB loss at the point, as a minimum-phase response so it has group delay. `width_mhz` is where the dB depth falls to 1/e |
| Frequency-selective loss | `location`, `shape` (`BandStop`, `HighPass` or `LowPass`), `f_lo_mhz`, `f_hi_mhz`, `depth_db`, `stop_return_loss_db`, optional `skirt_mhz` | Adds shaped minimum-phase loss, covering traps, filters, pullout and under-rated parts. Low return loss in the stop band also adds a mismatch. `skirt_mhz` is the width over which loss ramps in, in dB, default 2 MHz: a trap has a sharp edge, and a roll-off or pullout a long one [PNM-HFC §5.2.6.10, §5.2.6.15]. RxMER then holds at its ceiling until the level gets low, and falls after, which gives the curve its rounded shoulder |
| Amp drift / failure | `amp_id`, `state` (`Drift { gain_delta_db, tilt_delta_db, compression_onset_dbmv: Option }` or `Failed`), `onset` (step/ramp) | `Drift` modifies that amp's output and, above the onset level, its distortion (§4.4); `Failed` means no output (an explicit state, because JSON cannot carry −∞) |
| Upstream ingress | `entry_location`, `f_mhz`, `bw_mhz`, `power_dbmv`, `schedule` (time-of-day profile), `burst: Option { duration_ms, rate_per_s }` | Injects noise at the point; funnels per §7.3. With `burst`, FEC uses peak noise while RxMER and SNR use time-average power |
| Downstream ingress | `entry_location`, `f_mhz`, `bw_mhz`, `power_dbmv`, `schedule` (time-of-day profile) | Injects noise at the point; reaches modems downstream of it, reduced by path loss, per §4.5. An entry on a drop reaches only that modem |
| Power supply sag | `psu_id`, `v_delta`, `schedule` | Lowers V_supply; brownouts per §6.3 |
| Bad drop | `modem_id`, `extra_loss_db`, optional `return_loss_db` | Adds loss (and optionally a mismatch) on one drop only |
| Resonant peak | `amp_id`, `f0_mhz`, `gain_db`, `width_mhz` | Adds `gain·exp(−((f−f0)/width)²)` dB at the amp's output, minimum phase. The amp's CIN worsens 2 dB per dB of local rise (§4.4's third-order slope, approx), so RxMER dips where the level peaks. **Oracle:** CIN 60 dB, 17 dB peak: CIN at f0 is 60 − 34 = **26 dB** |
| Adjacent level offset | `f_lo_mhz`, `f_hi_mhz`, `offset_db` | A block's level at the node's output moves by `offset_db`. All below inherit it. A high block costs its OFDM neighbour through §4.5.1, and a low block loses CNR on its own |
| Forward optics | `cnr_loss_db` | Lowers `CNR_opt` by `cnr_loss_db`, a node-wide loss with no level change: a weak laser, a dirty connector, a clipping transmitter in noise terms. Analog nodes only. **Oracle:** 48 dB less 20 dB is **28 dB** |
| Common-path distortion | `location`, `second_order_dbmv_per_mhz`, `third_over_second_db`, `schedule` | Enters the return path at the point, as ingress. The shape is computed: the second-order product density is the autocorrelation of the forward channel plan's occupied spectrum, normalized to 1 at its largest, and the third order is that convolved with the plan again [PNM-HFC §8.16.1.1]. Each 6 MHz SC-QAM makes a peak at multiples of its spacing. The level is the parameter, since it's how corroded the contact is, and it's not computable. **Oracle:** two occupied bands of 5 MHz, 6 MHz apart: density at 0 is 10 MHz, at 6 MHz is 5 MHz, so −3.01 dB at 6 MHz |

Water on a drop works as on a span. Fully wet foam can move impedance far more than a few percent, so `impedance_variation_pct` runs to about 100 % (approx).

Compound faults: any set of the above; the sim applies them all. Domain
randomization: jitter every datasheet parameter by its tolerance (typically
±0.5 dB on losses, ±1 dB on amp gain, ±10% on R_loop) per seed.

---

## 9. What the sim emits (observation schema, per time step)

Channel records describe themselves, because channels move during migration and modem
generations see different subsets [CM-OSSI4 Table 91]. Each is keyed by a stable
`channel_id`. Subcarrier arrays carry `zero_freq_mhz`, `first_active_index` and
`spacing_khz`, and each record carries its `profile_ref`.

Unless grouped otherwise, these are per modem. Values are absolute, as a device reports
them. The feature layer (`docs/DESIGN.md`) derives the relative values models read.

**Downstream**

- `rxmer_db[subcarrier]` (§4.5 per subcarrier), with `rxmer_mean_db`,
  `rxmer_stddev_db` and `rxmer_p2_db` per OFDM channel (§5.5)
- `fbc_dbmv[bin]` full-band capture, carrying first bin center, bin width and its
  noise bandwidth, because a bin level isn't per 6 MHz
- `ds_chan_est[channel]`: per-subcarrier complex coefficients with a
  `ResponseSummary` (§5.5)
- `ds_rx_dbmv[channel][band]`: SC-QAM has one value. OFDM has one per 6 MHz band plus
  the PLC band, computed from active subcarriers [CM-OSSI]
- `codewords{total, corrected, uncorrectable}[channel][profile]` per interval
  (§4.6; SC-QAM channels have one profile)
- `plc{total, unreliable}[channel]` and `ncp{total, crc_failures}[channel]` per
  interval, OFDM only (§4.6)

**Upstream**

- `preeq`: 24 complex taps with `main_tap_index` and derived `mtc_db`, `mtr_db`,
  `nmter_db` (SC-QAM, §5.3) and/or per-subcarrier complex coefficients with a
  `ResponseSummary` (OFDMA, §5.4, §5.5)
- `us_tx[channel]`: `psd_dbmv_per_1p6mhz`, plus `total_dbmv` on SC-QAM, since a 3.1
  modem's SC-QAM total-power object stays total [CM-OSSI]
- `us_rx_dbmv_per_1p6mhz[channel]`: as the CMTS receives this modem (§7.2)
- `us_rxmer_db[subcarrier]` per OFDMA channel, with its channel mean `us_rxmer_mean_db` (§7.3)

**Modem state**

- `generation` (D3.0 or D3.1), so a reader knows which fields to expect
- `reg_state`: registration state (§7.4)
- `ds_channel_state[channel]` and `us_channel_state[channel]`: usable or not, which
  is partial service in each direction [MULPI §8.4]
- `partial_channel[ofdm_channel]`: profile, NCP and PLC bits (see `docs/reference/mac-operations.md`)
- `t3_timeouts[channel]`, `t4_timeouts[channel]`: per upstream channel and cumulative,
  like the real counters [CM-OSSI §F.2.3.2]
- `ranging_status[channel]`

**Per service group**

- `us_snr_db[channel]` (§7.3)
- `us_spectrum_dbmv[port][bin]`: upstream spectrum capture of `N_node(f)` and carriers,
  the localization method in §7.3
- `us_codewords{total, corrected, uncorrectable}[channel]` per interval, as the CMTS
  counts them; burst faults show here

**Shared types**

- `ResponseSummary`: `mag_mean_db`, `mag_slope_db_per_mhz`, `mag_ripple_rms_db`,
  `mag_ripple_pp_db`, `gd_mean_ns`, `gd_slope_ns_per_mhz`, `gd_ripple_rms_ns`,
  `gd_ripple_pp_ns`

**Per active**, where the hardware reports it: `v_in`, `i_draw`, `temp_c`, `status`, and
`out_level_dbmv` with `f_ref_mhz`, because no real source gives level at `f_hi`. HMS
reports a pilot, and SCTE 283 a total composite or a test point. SCTE-HMS amps have no
`temp_c` or `status` objects; SCTE 283 has both.

The sim emits a state or value only where a real device could report it, and each field
says which device reports it in `docs/reference/collection-management.md`.

Capability masks remove fields a given modem and CMTS could not report, using the
profile's set intersected with the modem's, and validity is decided when a value is emitted, since later profiles make
it depend on conditions (§11). The schema is versioned; see `docs/tasks/01-schemas.md`.

---

## 10. Environment

Temperature, rainfall and simulated time are inputs, not derived values and not
faults. Each is a sample in the environment series, applied to one plant. Sample spacing
sets the sim's time step (§7.4):

- `t`: simulated time, in seconds
- `temp_c`: air temperature, for aerial spans in §1.2 (no per-span variation in v1)
- `ground_temp_c`: for underground spans in §1.2
- `rain_mm_per_hr`: rainfall for the water-ingress term in §8

Time-of-day `schedule` values on faults (§8) are evaluated against `t`. The sim reads
these values only from the input series, never from a clock or a weather service.

---

## 11. Sim architecture that later profiles depend on

These are cheap to get right at steps 3 and 4, and expensive to retrofit for DOCSIS 4.0.
See `docs/reference/docsis40.md`.

- **Port-to-port paths.** Implement `transfer(port_a, port_b, f_mhz)` over the tree in
  either direction; §3.2 and §7.1 become special cases. It also carries off-path echoes
  (§5.1), and FDX's modem-to-modem coupling later.
- **Direction per element and modem.** Walks ask `carries(element, f_mhz, direction)`,
  because the band plan can differ by modem generation and overlap under FDX. Never
  assume one upstream band below one downstream band.
- **Output level as a function.** All uses of §3.1 go through `out_level(active, f_mhz)`.
- **Validity at emit time.** A value is `None` when its condition doesn't hold, as well
  as when a static capability mask removes it.
- **Errors outside rated bands.** A table lookup outside a component's rated band is an
  error (§2.1).

Deferred until FDX: the resource-block scheduler, echo cancellation, and traffic demand
as an input (which would extend invariant 1). A profile selects among implemented
duplex mechanisms, so the first FDX profile needs new code; later ones are data.

---

## 12. Validation without real data

No real measurements are available yet, so published results are the only outside check.
The suite runs at step 7 and has to pass before step 12 trains anything. Passing shows
the sim reproduces what CableLabs measured in those setups, not that it matches any
particular plant. Figure readings are approximate, so tolerances are generous.

| Target | Published result | Tolerance |
|---|---|---|
| OFDM uncorrectable curve | 50% near 23.45 dB at 256-QAM and 34.45 dB at 4096-QAM [PNM-HFC Fig. 74] | ±1 dB |
| OFDM correctable curve, 256-QAM | 50% near 6.8 dB of margin [PNM-HFC Fig. 76] | ±1 dB |
| Narrowband ingress on 4096-QAM | Uncorrectables near 8–9 dB in 192 MHz and near 15 dB in 96 MHz [PNM-HFC §V.3.2] | ±2 dB |
| Reflection, −10 dBc at 0.5 µs | Taps 10 and 11 at −17.1 and −14.4 dB, NMTER −11.61 dB [PNM-HFC Table 22] | ±2 dB per tap, ±1 dB NMTER |
| Band-edge group delay, N+5 | NMTER −15.9 dB with no reflection [PNM-HFC Table 12] | ±3 dB, once the `group_delay_ns` tables are set |
| Echo ripple | −10 dB at 500 ns gives 5.69 dB p-p, 2 MHz; 8 MHz at v_f 0.87 gives 53.5 ft [PNM-HFC §5.3.4.2] | ±0.1 dB, ±0.5 ft |
| RxMER on healthy plant | Average near 41.2 dB in the guide's example [PNM-HFC §5.3.4.3] | ±2 dB |
| Analog link CNR (§16), task 22 | The source's worked example: RIN 62.05, EDFA 56.78, shot 56.99, thermal 65.14, link 52.99 dB in 4 MHz [SCTE 270 §25.7.6] | ±0.02 dB |
| Analog return laser (§16), task 22 | One tested DFB: 35.5 dB at 7 dBmV total input, 52.5 dB at 26 dBmV [TR-MIDSPLIT §6.1.1.1] | ±0.5 dB, with a part fitted to it |
| Digital link budget (§16), task 22 | 10GBASE-LR: −8.2 to +0.5 dBm launch, −14.4 dBm sensitivity [IEEE 802.3 Cl.52] | the margin to 0.001 dB |
| Fiber delay (§16), task 22 | 500 µs per 100 km [TR-DCA §11.9.3] | exact |

**Sensitivity.** Each value marked approx or `Generic` is swept across a plausible range
while diagnosis runs on the same faults. The report ranks them by how much they change the
answer, which says which estimates matter and what to measure first when real data exists.

---

## 13. Premises

Version 2 plants carry the home (`docs/tasks/21-as-built-schemas.md`). The walk goes on past the tap port: drop, ground block,
in-home splitter legs and cable, drop amp, filter, device. Nothing here changes a plant without these parts.

- **Loss.** Each part adds its loss by frequency, read from its table as §2.1 reads a tap's: a ground block from input to output, a
  splitter to each leg. Cable is §1's. **Temperature:** a drop follows `temp_c`, as in version 1, unless it's buried, when it follows
  `ground_temp_c` (§1.2). In-home cable sits at `T_indoor` = **20 °C** (approx), a named constant.
- **Reflections.** Each part is a reflector with its own return loss: its input facing down and each output facing up, as §5.1 already
  treats a tap. So cavities form inside the home, and between the home and the tap port, under §5.1 unchanged. A device (modem or
  `Cpe`) is a reflector facing down. A `Cpe` reports nothing.
- **Two devices behind one splitter** see each other through its isolation, the off-path echo of §5.1 with the tap's isolation
  replaced by the splitter's `leg_isolation_db`. For a device on leg `m`, each other leg `n` gives one echo from the first part on
  `n`, at the end of a cable of length `d_n`:

        E_mn = −(ISO + RL_n + 2·A(f)·d_n/100) dB,   τ_mn = 2·d_n / (v_f·983.6) µs

  The leg losses cancel, since a splitter has one table for every leg. Only a splitter that states `leg_isolation_db` makes these
  echoes; version 1's splitters don't.
- **A filter** is a shaped loss and a stop-band mismatch, the mechanism §8's frequency-selective loss uses, here as a healthy part.
  Its loss is `pass_loss_db` everywhere plus `stop_depth_db` across each stop band, ramping in over `transition_mhz` at an edge that
  borders a pass band, with its minimum-phase phase (§8). An absent `transition_mhz` takes §8's 2 MHz edge (approx). Its return loss
  is `return_loss_db` in the pass band and falls to `stop_return_loss_db` in proportion to the stop-band loss, as §8's stop band does.
- **A drop amp** has gain and a noise figure each way, and is powered in the home, so it's outside §6. Forward, the level after it is
  the level before plus `gain_db`, and its noise enters §4.5 at its own place, from its own input level:

        CNR_da(f) = L_in(f) − NF_da − N_floor(6 MHz),   1/MER_plant += 10^(−CNR_da/10)

  Return, it adds `return_gain_db` to the path where the return is amplified, and its noise `kT·NF_ret·G_ret` enters §7.3 at its place,
  reaching the node as ingress does. A passive return path costs `return_passive_loss_db`. An amp is directional, so a drop amp ends
  one cavity section and starts another, as §5.1 says of any active (approx for the passive return path, which isn't).
- **Upstream**, the modem transmits through the same parts (§7.2): `Loss_up` gains the drop, the home's losses and a drop amp's
  return term.

**Oracle (the fixture's splitter home on `one_street`).** `fixtures/one_street_as_built/`'s home 2 (`drop-2` 140 ft series 6 aerial,
`gb-2`, `house-2a` 6 ft, `hsp-2`, `house-2b` 25 ft to `modem-2`, `house-2c` 40 ft to `stb-2`) hung on `one_street` low-split's
`tap-1` port 2, in place of its 150 ft drop, with its home 8 on port 3 (`drop-8` 90 ft, `gb-8`, `house-8a` 6 ft, `hsp-8`, `house-8b`
20 ft to `modem-8a`, `house-8c` 35 ft to `modem-8b`) and its home 3 on port 4. 20 °C, no fittings (those are §14's). `one_street`
because its amps hold §3.1's targets, so only this section is under test.

| Step at 750 MHz | dB | Level (dBmV) |
|---|---|---|
| `amp-1` output, §3.1 | | 46.28 |
| 400 ft .500, A = 1.983 dB/100 ft | −7.93 | |
| `tap-1` port, 23 dB | −23.00 | **15.35** at the port |
| `drop-2`, 140 ft series 6, A = 5.615 dB/100 ft | −7.86 | |
| `gb-2`, 0.2 dB at 55 to 0.5 dB at 1000 MHz | −0.42 | |
| `house-2a`, 6 ft | −0.34 | |
| `hsp-2` leg, 3.5 to 4.0 dB | −3.87 | |
| `house-2b`, 25 ft | −1.40 | **1.46** at `modem-2` |

Upstream at 30 MHz (A = 0.341 and 1.107 dB/100 ft; the ground block and splitter hold their 55 MHz values below it): 1.37 + 23.0 +
1.55 + 0.20 + 0.07 + 3.50 + 0.28 = **29.96 dB**, so `P_tx` = 8 + 29.96 = **37.96 dBmV** per 6.4 MHz (§7.2), reported as
**31.94 dBmV/1.6 MHz**.

In-home echoes at 750 MHz, with the splitter's 22 dB isolation and series 6's v_f of 0.85:

| Echo | Reflectors | τ (µs) | E (dB) |
|---|---|---|---|
| `modem-2`'s own cavity, `house-2b` | `hsp-2` leg, 18 dB; `modem-2`, 18 dB | **0.0598** | −(18 + 18 + 2·1.404) = **−38.81** |
| `modem-2` sees `stb-2` | `stb-2`, 10 dB, 40 ft | **0.0957** | −(22 + 10 + 2·2.246) = **−36.49** |
| `modem-8a` sees `modem-8b` | `modem-8b`, 18 dB, 35 ft | **0.0837** | −(22 + 18 + 2·1.965) = **−43.93** |
| `modem-8b` sees `modem-8a` | `modem-8a`, 18 dB, 20 ft | **0.0478** | −(22 + 18 + 2·1.123) = **−42.25** |

The last two are the same pair seen from both sides: each modem sees the other's cable, so the delays differ.

**Oracle (the drop amp, home 3 on the same tap's port 4).** At 750 MHz: 15.35 − 4.21 (`drop-3`, 75 ft) − 0.42 − 0.56 (`house-3a`,
10 ft) = **10.15 dBmV** in, + 10 dB gain − 1.12 (`house-3b`, 20 ft) = **19.03 dBmV** at `modem-3`, over the +15 dBmV window: a drop amp
on a short drop off a 23 dB tap overdrives (§3.3). Its noise: CNR_da = 10.15 − 5 + 57.42 = **62.57 dB**. Upstream at 30 MHz, its
passive return costs 1.5 dB: `Loss_up` = 1.37 + 23.0 + 0.83 + 0.20 + 0.11 + 1.50 + 0.22 = **27.23 dB**, `P_tx` = **35.23 dBmV**.

---

## 14. Fittings, port states and return loss by frequency

- **A fitting** (connector, splice or adapter) on an edge is a small flat loss, its `insertion_loss_db`, and a reflector at its
  `pos_ft` facing both ways, as a fault in the line is (§5.1). Fittings at an edge's two ends sit where the ports are, so they make no
  cavity with the port's own reflector (τ = 0). Unlike a fault's, a fitting's echoes are dropped below §5.1's healthy floor.
- **A power inserter** in the line has the insertion loss its part states, by frequency, and reflects at its `return_loss_db`. §6.2
  spoke only of its resistance; a part that states no loss still has none, as in version 1.
- **Return loss by frequency.** A part's `return_loss_table` is read in place of its single number, interpolated linearly in dB and
  held flat past its ends (§2.1). A tap reads `return_loss_by_port` for the class of port the reflector is at: its input, its through
  port or a tap port. Without a table, the single number holds, as before.
- **Port states.** A port with nothing on it and a state reflects: an `Open` port totally (RL = 0 dB, approx), a `Terminated` one at
  its terminator's return loss. Seen from the passive's input, a stated port is one more reflector facing down there, behind the
  passive's own loss to that port twice over:

        RL_eff(f) = RL_state(f) + 2·L_port(f)

  So an open port on a low-value tap is a strong mismatch on the line [PNMP §5.2.1.2] (plant-and-powering.md §4), and it forms cavities
  with reflectors above it under §5.1. The same wave also reaches the passive's other ports through its isolation, with no cable
  between: a zero-delay echo, which is a level change of under 0.3 dB at 30 dB of isolation and no ripple. That's left out (approx).
  A port with neither an edge nor a state is matched, as in version 1.
- **A terminating tap** is a tap with no through port, so §2.1 prices it unchanged: its tap loss to each port and its input's return
  loss, which is the end of the line's reflection.
- **The floor.** More reflectors mean many more pairs. §5.1's per-pair floor stays: a healthy pair below −50 dB is dropped, a pair with
  a fault below −80 dB. The walk counts what it drops, per modem, as the summed power of each dropped echo at §5.1's 100 MHz reference,
  relative to the main signal. On the whole as-built fixture at 20 °C, either profile, each modem's dropped echoes sum to **−38.5 to
  −41.7 dB** (`modem-1`, the first tap's, −41.75), and the 7 to 19 echoes it keeps to −28.5 to −32.4 dB: what the floor drops is 9 to 12
  dB below what it keeps. Dropped echoes aren't summed into a noise term, for the reason §4.5 gives.
- **Keeping homes cheap** (task 22's latency bar). A home adds about nine kept echoes to each modem's path, each costing a power, a
  sine and a cosine at every grid frequency. Where a home's reflector takes part (anything below its ground block, filter, drop amp or
  set-top, on house cable, or a fitting), the echo is computed exactly every 32 points of an evenly spaced stretch of the grid, with
  its amplitude linear and its phase rotated in between (approx, numerical: within 1e-6 of the exact response on the fixture's homes,
  `oracles.rs`). Echoes between version 1 reflectors stay exact at every frequency, so a plant without new parts reads to the same
  bytes. And the walk doesn't descend where nothing reads: a subtree with no modem, active, ground block or fault (a set-top's leg, a
  terminator) changes no view, capture or noise.

**Oracle [SCTE 183 §9.1].** Two reflectors of 7 dB return loss 100 ft apart on .500, at the frequency where .500 loses 1 dB per 100 ft
(224.80 MHz): `E` = −(7 + 7 + 2·1) = **−16 dB**, τ = 200/(0.87·983.6) = **0.2337 µs**. On `one_street` low-split, as two 7 dB
connectors at 100 and 200 ft on `span-3`, every modem below `tap-2` carries that echo.

**Oracle (an open port on a low-value tap).** `one_street` low-split with `tap-4` an 8 dB 4-port tap (`tap-generic-8db-4p`) and its
port 3 `Open`. At its input that port reads RL_eff = 0 + 2·8 = **16 dB**, and makes a cavity with `tap-3`'s through port (18 dB) across
`span-6`'s 300 ft. At 750 MHz: `E` = −(16 + 18 + 2·5.948) = **−45.90 dB**, τ = 600/(0.87·983.6) = **0.7012 µs**, seen by `modem-7`
and `modem-8`. `tap-4`'s own 18 dB input makes the same cavity at **−47.90 dB**.

**Oracle (fittings and an inserter in the level).** Each `.500` pin connector costs 0.05 dB, so the as-built fixture's two per span
take 0.10 dB a span. `pi-generic-1218` loses 0.4 dB at 55 MHz, 0.5 at 1000 and 0.9 at 1218: **0.474 dB** at 750 MHz.

---

## 15. Alignment

Replaces "a healthy active holds its target" (§3.4) where an active has `fitted`. An active without `fitted` behaves exactly as
before.

**Forward.** Per output port `p` of a fitted amp, with `L_in` its input (§3.2):

    L_out,p(f) = L_in(f) − P_in − EQ_in(f) + G(f) + AGC(f) − L_main,p − P_p − EQ_p(f)

`P` is a pad's value plus its insertion loss. `G(f)` is the part's `gain_table`, or `max_gain_db` flat where it has none. `L_main,p` is
the port's `loss_from_main_db`, for any amp that lists `outputs`, fitted or not. A plug-in's loss is its `loss_table` where it has one,
and otherwise a shape across its `slope_band` from `lo` to `hi`, held flat beyond it (approx, the cable's √f shape [practice]):

    equalizer:        L(f) = IL + V·(√hi − √f)/(√hi − √lo)        V more loss at lo than at hi
    cable simulator:  L(f) = IL + V·(√f − √lo)/(√hi − √lo)        V more at hi
    pad:              L(f) = IL + V

A tap's faceplate conditioner is an equalizer on every tap port, and a line equalizer is one in its own housing, cut into a span. On a
diplexed line equalizer the shape is forward only, and its return path costs its insertion loss alone (approx).

- **AGC** reads the input at the part's highest `agc_pilots_mhz`, or the band's top where it has none, and corrects the deviation from
  the input at alignment, within its range, shaped like the section's cable (§3.4). It acts on the gain.
- **Set point.** `set_point` gives the station's own target, `out_dbmv_at_hi` with its `tilt_db` across the forward band as §3.1 does;
  without one, the profile's. A fitted root holds its set point, or the profile's target: its plug-ins are how the node reaches it, so
  only a change to them (a misalignment, `fault`) moves it (approx).
- **What alignment leaves.** The deviation an aligned station has at reference conditions (20 °C, healthy) is its *residual*, the part
  plug-in steps and shapes can't remove. It travels with the signal, and the next active's AGC reads against it, not against zero.

**`sim::align(plant, library, profile)`** is the pass a technician makes. Working out from the root, depth by depth, at 20 °C with no
fault: for each fitted amp, from its input at the pilots (the forward band's two edges, or its two pilots), it picks from the plug-ins
of its family the input pad and equalizer that minimise the summed squared error against the set point at those two frequencies, then
each output's pad the same way. Ties go to the smaller values. It's pure and deterministic, and returns fitted values; it changes no
plant.

**Equalizer shapes and the sweep (task 25).** A plug-in may state its loss as a measured `loss_table` in place of a value and a shape.
The generic family holds, beside its √f equalizers, ones whose table follows the `.500` feeder's own attenuation, `a·√f + b·f` with
a = 0.0598 and b = 0.00046 dB/100 ft (`cable-generic-500`), across the same slope band [inferred]: a real cable equalizer is made to
match the cable it equalizes, and the √f shape does that only for the skin-effect term. Over a span, the `b·f` term and taps' through
loss, which rises about linearly, leave a √f equalizer about 1.5 dB of bow an amp. `sim::align` picks among the equalizers a value and
a shape state, so the pass above is unchanged: two pilots can't tell the shapes apart. `sim::align_holding(plant, library, profile,
held)` is the same pass with the input equalizer of each amp in `held` given rather than chosen, and the amp taking only the pad that
suits it. The designer's sweep (task 25) uses it to try equalizers of either shape on an amp outside N/10 + 3 dB and re-align what's
below.

**Oracle (the two shapes at 558 MHz).** 10 dB, insertion loss 0.8 dB, slope band 54 to 1218 MHz. √f: share = (√558 − √54)/(√1218 −
√54) = (23.6220 − 7.3485)/(34.8999 − 7.3485) = 0.59066, loss 0.8 + 10·(1 − 0.59066) = **4.893 dB**. Two-term: C(f) = 0.0598·√f +
0.00046·f, so C(54) = 0.46428, C(558) = 1.66928, C(1218) = 2.64729; share = 1.20500/2.18301 = 0.55199, loss 0.8 + 10·0.44801 =
**5.280 dB**. Both are 10.8 dB at 54 MHz and 0.8 dB at 1218.

**Return.** Pads and equalizers set unity gain back to the reference (§7.1). A return signal from port `p` crosses the combiner, the
return amp, its pad and equalizer, and the section above (loss `L_above`, at reference). Alignment picks the return pad and equalizer
for the port with the largest combining loss [SCTE 183 §16.4] (plant-and-powering.md §7.1), at the return band's two edges. What's
left adds to the path's excess (§7.1):

    residual_p(f) = L_above(f) − (G_ret − P_ret − EQ_ret(f) − C_p)

and the amp's own noise reaches the node at its gain from there, `kT·NF_ret·(G_ret − P_ret − EQ_ret − L_above)`, which is `C_max` above
an unpadded amp's when aligned: the combining loss is in the noise funnel too [SCTE 183 §16.4]. An amp that lists `outputs` and isn't
fitted is aligned for its largest combining loss by assumption, so its other ports run `C_max − C_p` hot and its noise is `C_max` up.

**Oracle (the fixture's first amp, low-split).** `amp-1` on `fixtures/one_street_as_built/` at 20 °C. The root holds the profile's
50 dBmV at 1002 MHz with 14 dB of tilt. Above `amp-1`: 1,200 ft of `.750` (A = 0.367 dB/100 ft at 54 MHz, 1.757 at 1002), four pins and a
splice at 0.05 and 0.1 dB, and `pi-generic-1218` (0.40 dB at 54 MHz, 0.504 at 1002):

| | 54 MHz | 1002 MHz |
|---|---|---|
| Root output | 36.00 | 50.00 |
| `.750`, 1,200 ft | −4.41 | −21.09 |
| Fittings, inserter | −0.70 | −0.80 |
| **`amp-1` input** | **30.89** | **28.11** |

`G` = 36 dB flat. The family's pads are 0 to 20 dB in 1 dB steps with 0.3 dB of insertion loss, and its equalizers 0 to 20 dB in 2 dB
steps over 54-1218 MHz with 0.8 dB. A value-`E` equalizer loses `0.8 + E` at 54 MHz and `0.8 + 0.1178·E` at 1002. Each output keeps at
least the family's smallest pad, `pad-generic-0db`, which still costs its 0.3 dB. The continuous best is E = 19.0, P = 10.5; of the
family's steps, **`eq-generic-20db` and `pad-generic-10db`** (squared error 0.381 dB²; the next best, 18 and 11, is 0.413). Both outputs
have no loss from the main, so each takes **`pad-generic-0db`**. Output and residual:

| MHz | Input | Output | Target | Residual |
|---|---|---|---|---|
| 54 | 30.89 | **35.49** | 36.00 | **−0.51** |
| 550 | 27.47 | **43.76** | 43.32 | **+0.44** |
| 1002 | 28.11 | **50.35** | 50.00 | **+0.35** |

Return, 5 to 42 MHz: `L_above` = 2.01 dB at 5 MHz and 4.57 at 42 (the inserter holds its 55 MHz loss below it). `G_ret` = 25 dB,
`C_max` = 3.5 dB, return pads 0 to 12 dB in 2 dB steps (0.3 dB insertion), return equalizers 0 to 6 dB over 5-204 MHz (0.5 dB insertion;
a value-`E` one loses `0.5 + 0.6477·E` at 42 MHz). The best is **`rpad-generic-12db` and `req-generic-6db`**, with a residual of
**−0.69 dB** at 5 MHz and **−0.24 dB** at 42: the family runs out at 12 dB of pad, so the port runs slightly hot.

**Check, not an oracle [SCTE 183 §6.2].** Peak to valley of the residual through the fixture's cascade, at `amp-2`'s output across the
forward band, stays inside `N/10 + 3` = **3.2 dB** for its two amps.

**Misalignment** (a fault, §8, fault version 4) is a wrong plug-in at an active, as its difference from what's fitted: `pad_delta_db`
more forward input pad, `eq_delta_db` more forward input equalizer, `return_pad_delta_db` more return pad. The forward deltas join the
input plug-ins' loss before the gain, the equalizer's on its √f shape across its slope band (the forward band without one), so the
station holds the wrong level and its own noise reads the lower input. On an unfitted active they move its output directly. The return
pad adds to the return path's excess below it (§7.1). **Check** (`sim/tests/fiber_faults.rs`): 2 dB more pad on `amp-1` is 2 dB less
output at every frequency; 4 dB more equalizer is 4 dB less at 54 MHz and 4·(√1218 − √1002)/(√1218 − √54) = **0.47 dB** less at 1002.

## 16. Optical links

A node's optics, from the fiber plant (`schema::fiber`) and what's fitted on the plant's root. `sim::optics` computes each node's link
state, and the sim reads it: an analog node's forward CNR and return SNR replace §4.5's `CNR_opt` and §7.3's `us_optics_snr_db`, and an
RPD whose link is down falls through `sim::rpd` as a CIN cut does. A node with no link keeps both constants as before, so a plant with
no fiber plant simulates to the same bytes.

**Where the documents meet.** A link's far end is on the coax plant. A forward link ends at a `NodeEnd` and is received by the root's
fitted `forward_rx` (analog) or its first `transceivers` entry (digital); a return link starts at a `NodeEnd` and is launched by the
root's `return_tx` or that optic. The hub end is the link's own element's part. A link whose node end has nothing fitted to receive or
launch it is an invalid plant.

**Budget.** A plain sum in dB over the link's hops [SCTE 270 §25.8.3-§25.8.4] (fiber-optics.md §3):

    P_R = P_T − Σ_fiber α(λ)·len_ft/1000 − Σ_splice L_s − Σ_connector L_c − Σ_through IL

- `α(λ)` is the cable part's loss at the link's wavelength, linear between the wavelengths it states and held at the end ones beyond them
  (approx). Length is the cable's sheath length, slack included.
- A splice takes its measured `loss_db`, or the `splice_loss_db` of the closure or panel it's made in. A splice with neither is an
  invalid plant.
- A connector hop is a mated pair, at its part's `insertion_loss_db`. Every connector the link lists counts, at the equipment too, as
  SCTE 270's worked example counts them [SCTE 270 §25.8.4].
- Through an optical passive: its `insertion_loss_db`, whichever port pair. Through an optical amplifier: its output is its `out_dbm`
  whatever the input (constant output power, approx), and a wavelength outside its band is an invalid plant.
- `P_T` is an analog transmitter's `out_dbm`. A pluggable launches at the middle of its stated range (approx).
- **Latency** is 500 µs per 100 km [TR-DCA §11.9.3], **1.524 µs per 1,000 ft** of fiber (our conversion). It's reported, and not yet fed
  to ranging (Deferred, task 22).

**Analog CNR.** Four terms, each in noise bandwidth `B`, combined in linear [SCTE 270 §25.7.1-§25.7.5] (fiber-optics.md §5.2):

    CNR_RIN     = 20·log10(m) − 10·log10(2B) − RIN
    CNR_shot    = P_R + 20·log10(m/√2) + 10·log10(R) − 10·log10(B) + 154.94
    CNR_thermal = 2·P_R + 20·log10(m/√2) + 20·log10(R) − 10·log10(B) − 20·log10(I_r) + 180
    CNR_EDFA    = 86.2 + P_in + 20·log10(m) − NF − 10·log10(B / 4 MHz)       per amplifier on the path, at its input P_in

`m` is the per-channel modulation index, `RIN` the transmitter's, `R` and `I_r` the receiver's responsivity and equivalent input noise,
`NF` the amplifier's. `B` is 6 MHz, the bandwidth `m` is stated per, so CNR is per 6 MHz on both sides of the link and combines with
§4.5's terms as they stand. The source's EDFA term is in 4 MHz, so it's moved to `B`; the others carry `B` already. The hub drives a
forward transmitter at its `ref_rf_in_dbmv` per channel, so `m` is its `omi_pct` (approx: the CMTS's drive isn't modelled). A node's
return laser is padded to its `ref_rf_in_dbmv` per 6 MHz across the loaded return band, as a node data sheet states its nominal return
input per 6 MHz [CS-OM4120] (fiber-optics.md §5.6), so `m` is the part's too (approx). §17 states the composite this makes.

**Clipping.** Light can't go below zero, so the negative peaks of the composite beyond the bias clip [SCTE 270 App. F]. A flat
composite of digital channels is close to Gaussian [SCTE 183 §16.2], with RMS index `µ = m·√(N/2)` for `N` equal channels [SCTE 270
App. F]. With `x ~ N(0, µ²)` the modulation and `a = 1/µ`, the clipped-off part is `−(1 + x)` where `x < −1`, and its power relative to
the signal's is, with `φ` and `Q` the standard normal density and tail (our derivation):

    ρ = (1 + a²)·Q(a) − a·φ(a) = φ(a)·∫₀^∞ t²·e^(−a·t − t²/2) dt         CNR_clip = −10·log10(ρ)

The integral form has no cancellation at large `a`; the sim integrates it by Simpson's rule. `N` is the loaded bandwidth in 6 MHz
channels, from the profile's channel plan: SC-QAM widths, and each OFDM or OFDMA channel's active span plus one subcarrier. Taking the
clipped power as white across the loaded band, and all of it in band, is approx: it's impulsive and spreads wider. It's the steep side of
the return's NPR curve, and on the forward it stays far down until a plan loads more channels than the drive was set for.

    1/lin(CNR_link) = Σ 1/lin(CNR_RIN, CNR_shot, CNR_thermal, CNR_EDFA…, CNR_clip)

**Analog return.** The same terms, from the node's `return_tx` to the hub's receiver, give the link's upstream SNR. Per channel it doesn't
depend on the channel's width, since signal and noise are both spread over the loaded band (NPR's notch reads the same ratio [SCTE 183
§13.5.2]). Against the laser's total input, SNR rises 1 dB per dB on the noise side and falls steeply on the clipping side, as NPR curves
do (plant-and-powering.md §8).

**Digital.** Margin is received power less the optic's `rx_sensitivity_dbm`. Below sensitivity, or above `rx_overload_dbm`, the link
is down, either way. An RPD with either link down is cut off as a `CinLinkCut` is (§11, `sim::rpd`): offline, RF off, its modems falling
through §7.4 on their own.

**A link that's down.** A cut makes `P_R` minus infinity, reported as no light. On an analog node a forward link down leaves the node
with no RF, as an RPD's RF off does; a return link down makes the upstream SNR minus infinity, so the CMTS hears nothing from the node
and every ranging opportunity is lost. Modems keep their downstream RxMER while they stay registered, lose each upstream channel after
§7.4's 17 misses, reinitialize, and can't register again until the return is back, since registering needs ranging.

**Fiber faults** (`schema::fiber_fault`, held per region, since one can reach several nodes). Each is physical parameters at a fiber
location, and `sim::optics` reads them into the budget above at the time asked, under the onset's share as §8 reads a coax fault's:

- **`FiberCut`** on a cable stops light on every strand through it, from its onset until `repair_s` later, so every link with a hop on
  that cable is down and no other.
- **`EventLoss`** adds its loss at a point on a cable (every strand through it), at an element (every link with a connector at it or
  passing through it, once per link), or at one splice, named by the two strands it joins.
- **`TransmitterDrift`** moves a transmitter's launch power and, on an analog one, its RIN: a hub transmitter or optic, or a node's
  return laser or RPD optic by its `NodeEnd`.

**What it doesn't do.** The node holds its RF output whatever the received power (§15, approx): a receiver outside its stated input
range moves CNR, and `check_physics` flags it, but its RF output isn't modelled. Dispersion, nonlinearity and second- and third-order
distortion are out of scope (task 22).

**Oracle (forward, low-split).** `link-low-fwd` on `fixtures/one_street_as_built/`: `ftx-generic-1310` (10 dBm, RIN −160 dB/Hz,
`m` = 0.0358) through the hub's 1×8 coupler to the node's `frx-generic` (R = 0.85 A/W, I_r = 7 pA/√Hz), 1310 nm.

| Hop | Loss, dB |
|---|---|
| Five SC/APC pairs at 0.3 | 1.5000 |
| `jumper-1`, `jumper-4`, 15 ft each at 0.305 dB/kft | 0.0092 |
| `osplit-1`, 1×8: three 50/50 stages at 3.40 dB [SCTE 270 §25.8.2] | 10.2000 |
| `fiber-1`, 1,700 ft at 0.122 dB/kft (G.652.D max) | 0.2074 |
| Splice in `closure-1`, no measured loss: the closure's 0.05 | 0.0500 |
| `fiber-2`, 1,300 ft | 0.1586 |
| **Total** | **12.1252** |

`P_R` = 10 − 12.1252 = **−2.1252 dBm**, inside the receiver's −8 to +1. Latency: 3,030 ft, **4.618 µs**. In 6 MHz:

| Term | dB |
|---|---|
| RIN: −28.922 − 70.792 + 160 | 60.286 |
| Shot: −2.125 − 31.932 − 0.706 − 67.782 + 154.94 | 52.395 |
| Thermal: −4.250 − 31.932 − 1.412 − 67.782 − 16.902 + 180 | 57.722 |
| Clipping: 32 SC-QAM and one 190.0 MHz OFDM, `N` = 63.667, `µ` = 0.2020, `a` = 4.951 | 75.959 |
| **CNR_link** | **50.751** |

So the node's forward optics cost about 1.6 dB of MER less than the 48 dB constant assumed for nodes with no link. The source's own worked
example, in 4 MHz with its EDFA (m = 0.0358, RIN −160, NF 5.5 dB at +5 dBm in, `P_R` = 0 dBm, R = 1.0, I_r = 7), is a second oracle: RIN
62.05, EDFA 56.78, shot 56.99, thermal 65.14, link **52.98 dB** (the source rounds to 52.99), and **55.33 dB** without the EDFA
[SCTE 270 §25.7.6] (fiber-optics.md §5.2).

**Oracle (return, low-split).** `link-low-ret`: the node's `rtx-generic-1310` (3 dBm, RIN −150, `m` = 0.10) to the hub's
`rrx-generic-hfc` (R = 0.8, I_r = 2.5). Three pairs 0.9, fiber 0.3660 + 0.0046, splice 0.05: loss **1.3206 dB**, `P_R` = **+1.6794 dBm**,
latency **4.595 µs**. RIN 59.208, shot 64.859, thermal 82.670; four 6.4 MHz SC-QAM, `N` = 4.267, `µ` = 0.1461, clipping 128.3: SNR
**58.147 dB**, where §7.3's constant assumed 40.

**Oracle (return anchors) [TR-MIDSPLIT §6.1.1.1].** One tested analog DFB return laser reads about 35.5 dB at 7 dBmV total input and
52.5 dB at 26 dBmV (plant-and-powering.md §7.4). `rtx-generic-1310-npr` is a generic part fitted to those two points: `m` = 6.9 % per
6 MHz at 12 dBmV, RIN −150.5 dB/Hz, into `rrx-generic` (R = 0.8, I_r = 2.5) at −6 dBm, with 80 MHz loaded. The received power is the
condition a node data sheet quotes its return NPR at [CS-OM4120], and the 5-85 MHz load is our reading of a mid-split test, so both are
approx. Per channel, `m = 0.069·10^((P_total − 10·log10(80/6) − 12)/20)`:

| Total in, dBmV | `m` | RIN | Shot | Thermal | Clipping | **SNR** | TR-MIDSPLIT |
|---|---|---|---|---|---|---|---|
| 7 | 0.01063 | 40.236 | 37.707 | 47.839 | — | **35.517** | 35.5 |
| 20 | 0.04747 | 53.236 | 50.707 | 60.839 | 173.3 | **48.517** | |
| 26 | 0.09471 | 59.236 | 56.707 | 66.839 | 56.912 | **52.541** | 52.5 |
| 27 | 0.10626 | 60.236 | 57.707 | 67.839 | 48.207 | **47.467** | |
| 28 | 0.11923 | 61.236 | 58.707 | 68.839 | 41.082 | **40.959** | |

Both anchors within **±0.1 dB**; the test holds them to ±0.5. The curve peaks near 25 dBmV and has fallen 12 dB by 28, which is where the
source's 7 to 27 dBmV usable range for 1024-QAM ends.

**Oracle (digital, high-split).** `link-high-fwd` and `link-high-ret`: `sfp-generic-10g-lr` at both ends, launching at −3.85 dBm (the
middle of −8.2 to +0.5). Three pairs, 3,015 ft of fiber and the splice: **1.3206 dB**, `P_R` = **−5.1706 dBm**, margin over −14.4 dBm
**9.2294 dB**, and 5.6706 dB below overload. Both up.

## 17. Source MER, receiver ceiling and loading

Each where the part states it; without it, as before.

- **`MER_src`** (§4.5) is read from the RPD's `source_mer`, or, behind an analog node, from the `source_mer` of the CMTS the fiber
  plant binds the service group to. The CMTS reaches the sim with the node's link state (§16). Without a table, PHY Table 41's floors
  as before. An analog node's own `source_mer`, what its RF stages allow with the optics excluded, as a node sheet states it
  [CS-OM4120] (fiber-optics.md §5.4), is one more term in §4.5's sum.
- **An upstream receiver ceiling**, an RPD's `us_rx_ceiling_db` or the bound CMTS's, combines in linear with §7.3's SNR and with the
  return optics:

      1/lin(SNR_us) = 1/lin(P_rx − N_node) + 1/lin(SNR_opt) + 1/lin(SNR_ceiling)

  It's the highest upstream RxMER the burst receiver can report, so upstream RxMER takes it too.
- **Noise figure and CIN by frequency.** An active's `nf_table` replaces `nf_db` at each frequency, and its `cin_table` replaces
  `cin_ref_db`, both linear between points and held flat beyond them, as every table is (§2.1).
- **Composite power.** Summed from the channel plan with the output tilt: per-6 MHz level `L(f) = L_hi − T·(f_hi − f)/(f_hi − f_lo)`,
  integrated across each channel's occupied band (an SC-QAM's width; an OFDM channel's active span plus one subcarrier, as §16 counts
  it):

      P_comp = 10·log10( Σ_ch ∫ lin(L(f)) df / 6 MHz )

  Where an active states `ref_composite_dbmv`, its CIN follows the composite rather than the level at the top frequency, at §4.4's
  1 dB per dB: `CIN(f) = CIN_ref(f) − (P_comp − P_comp,ref)`, and compression (§8) still reads the top level. `L_hi` is
  the active's actual top level; the tilt is the profile's design tilt, so a station with a set point of its own tilt is read at the
  design tilt (approx).
- **Optical modulation depth.** The hub drives a forward transmitter flat at its reference per channel (§16), so the composite at its
  input is `ref + 10·log10(N)`, which is what sets §16's `µ = m·√(N/2)`. A transmitter whose AGC holds the composite would lower `m` as
  channels are added; no part states the loading its index is quoted at, so that's left out (approx).

**Oracle (the §4.5 modem again).** `modem-4` on `one_street` high-split at 750 MHz, 20 °C, with three tables added in the test (inputs,
not library values): the RPD's `source_mer` 49 dB at 258 MHz and 46 at 1218; `amp-1`'s part with `nf_table` 6.5 dB at 54 MHz and 9.5 at
1218, `cin_table` 61 dB at 54 and 57 at 1218, and `ref_composite_dbmv` 66.

| Term | By hand | dB |
|---|---|---|
| `MER_src` | 49 − 3·(492/960) | **47.462** |
| `amp-1` CNR | 22.31 − (6.5 + 3·696/1164 = 8.294) + 57.419 | **71.435** |
| Composite at `amp-1`'s output | 32 SC-QAM and two OFDM at 52 dBmV at 1218 with 12.1 dB of tilt | 68.561 dBmV |
| CIN | (61 − 4·696/1164 = 58.608) − (68.561 − 66) | **56.048** |
| `CNR_cm` | 7.24 − 10 + 57.419 | 54.658 |
| `MER_impl` | | 44 |
| **MER** | | **41.957** |

Against §4.5's **41.88** with none of the tables: the cleaner source more than pays for the noisier amp and the heavier loading.

**Oracle (receiver ceiling).** §7.3's high-split RPD reads 10 + 44.13 = **54.13 dB**; with `us_rx_ceiling_db` 50 on the RPD,
−10·log10(10^−5.413 + 10^−5) = **48.58 dB**.

## 18. Upstream baseline

A healthy home's wiring picks up some of the radio around it, and the return path carries it to the node. That's a property of the
plant at rest, not a fault, and it's what gives a healthy node's upstream a baseline above the amps' thermal noise (§7.3).

- **Ambient sources** are part of the environment (`Environment::ambient_sources`): each a band `lo_mhz`-`hi_mhz`, a level
  `level_dbmv_per_mhz` that wiring with no shielding at all would pick up, and an hourly schedule that multiplies its power, as a
  fault's does (§8). They act on the return band only; pickup a downstream receiver hears is a fault's (`DownstreamIngress`).
- **Coupling.** A ground block's `coupling_db` says how far below that level the home's wiring sits. Its product enters the return
  path at the ground block, on the home's side of it, as continuous noise:

      N_g(f) = level_dbmv_per_mhz − 60 + 10·log10(schedule(hour)) − coupling_db        dBmV/Hz, in the band

  and reaches the node at the path's return loss from there, `Loss_up + excess` (§7.1), funnelling with every other home's under §7.3:

      N_node = N_coax + Σ_g lin(N_g − reach_g)

  A home with no ground block, or one with nothing fitted, picks up nothing (approx: its coupling isn't known).
- **A fault that opens a shield** lowers coupling at its own place: a `ConnectorReflection` with `ambient_coupling_db` lets ambient in
  at that point at that coupling, through the same sum. One defect commonly shows echo, ingress and leakage together [PNM-HFC §9.7]
  (impairments.md §12). No public source gives a level for typical ambient pickup or a home's coupling, so both are inputs, and the
  fixture's 60 dB is `Generic`.

**Oracle (three homes under one source).** §13's homes 2, 8 and 3 on `one_street` low-split's `tap-1`, with `gb-2`, `gb-8` and `gb-3`
at 55, 40 and 30 dB of coupling (test inputs) and home 4's ground block with nothing fitted. One source, 5 to 42 MHz at 0 dBmV per MHz,
its schedule at 4 for the sample's hour (+6.02 dB). At 30 MHz, with §13's upstream losses (`.500` 0.3413 dB/100 ft, series 6 1.1066,
the ground block's 0.20):

| Ground block | Reach to `amp-1`, dB | `N_g`, dBmV/Hz | At the node, per 6.4 MHz |
|---|---|---|---|
| `gb-2`, 140 ft drop | 1.3654 + 23 + 1.5492 + 0.20 = **26.115** | −60 + 6.021 − 55 = −108.979 | **−67.03** |
| `gb-8`, 90 ft | 1.3654 + 23 + 0.9959 + 0.20 = **25.561** | −93.979 | **−51.48** |
| `gb-3`, 75 ft | 1.3654 + 23 + 0.8300 + 0.20 = **25.395** | −83.979 | **−41.31** |
| **Sum** | | | **−40.90** |

The amps above are at unity, so `amp-1` is the node for these losses. With §7.3's `N_coax` of −44.13 dBmV, `N_node` = **−39.21 dBmV**,
and a modem received at 8 dBmV reads **47.21 dB** where it read 52.13. The loosest-coupled home sets the baseline, 10 dB over the next.

## 19. Environment

What the sample does to plant beyond §1.2's two temperatures and §8's rain. A sample with no wind and no sun, and parts that state no
coupling, simulate exactly as before.

- **Sun on aerial plant.** An aerial span or drop runs warmer than the air in sunlight:

      T_aerial = temp_c + k_sun·solar_w_per_m2,   k_sun = 0.02 °C per W/m²

  so full sun at 1,000 W/m² adds 20 °C (approx: our estimate for a dark jacket in light wind; no source in our set gives a figure,
  and wind cooling isn't modelled). It moves attenuation through §1.2's `k_temp` and loop resistance below, everywhere §1.2's
  temperature is read. Buried and in-home cable see no sun.
- **Loop resistance rises with temperature** (§6.2), where the cable part states `r_loop_k_temp`:

      R_loop(T) = R_loop,20 · (1 + r_loop_k_temp·(T − 20))

  at the span's own temperature, its exposure's (§13). A part that states none holds `r_loop_ohm_per_kft` at every temperature, as
  version 1 did. Self-heating from the current isn't modelled (approx). `solve_ac` without a sample reads 20 °C, where nothing moves;
  the sim and `check_physics` solve at each sample.
- **A connector fault's couplings** (`ConnectorReflection::couplings`) move its return loss with the series, as water's rain
  coupling moves its loss (§8), so intermittency comes out of the series instead of being scripted:

      RL(sample) = RL_0 + temp_db_per_c·(T − 20) + wind_db_per_m_per_s·wind_m_per_s,   held at 0 dB or more

  `T` is the temperature at the fault's place: a span's own, or at a port the temperature of the edge into that element (indoor for a
  house cable, §13). The linear form is approx: rapid reflection swings may mark a loose connector [PNM-HFC §8.11.4]
  (impairments.md §12), but no source gives a coupling.

**Oracle (the §6.2 feed at 45 °C).** §6.2's 1,500 ft of `.500` with R_loop 1.7 Ω/kft at 20 °C, two 60 W amps at 90 V, now with
`r_loop_k_temp` 0.004 per °C (test input), aerial at 30 °C air in 750 W/m² of sun: T = 30 + 0.02·750 = **45 °C**, R_loop =
1.7·(1 + 0.004·25) = **1.87 Ω/kft**. Pass 1: I = 1.333 A, ΔV = 1.333·1.87·1.5 = 3.74 V, V = 86.26. Pass 2: I = 1.3911, V = 86.098.
Pass 3: V = 86.090, then 86.090 to the millivolt: **86.09 V**, 0.37 V below §6.2's 86.46 at 20 °C.

**Oracle (a coupled connector at two samples).** `RL_0` 14 dB, `temp_db_per_c` +0.2 (it loosens as it cools), `wind_db_per_m_per_s`
−0.8. At 20 °C and calm it's **14 dB**. At 20 °C in 10 m/s of wind, **6 dB**, so every echo it makes is 8 dB stronger at the same
delay, the cable unchanged. At −5 °C in 10 m/s, 14 − 5 − 8 = **1 dB**; at −30 °C in 15 m/s the sum is −8 and it's held at **0 dB**.

**What `check_physics` adds (task 22).** Each reads what the sections above compute; none changes a number.

- **An inserter port's current** (§6.2, §14): the actives beyond each of its ports, at the sample where they draw most, against the
  part's `port_i_rated_a`.
- **A plug-in beyond its family** (§15): a misalignment's deltas leave a pad or equalizer at a value its host's family doesn't offer for
  that slot. Which family and slot a fitted part may fill is checked when the plant is read (task 21).
- **Optical margin** (§16, `sim::optics::link_issues`): received power outside the receiver's window (an analog receiver's input range,
  an optic's sensitivity and overload), or no light; and a return laser whose drive, its reference per 6 MHz across the loaded return,
  passes the composite its part says it clips at.
- **A filter in the way** (§13): a filter whose stop band covers a channel a device below it uses, a modem both ways and a set-top or
  television downstream, which `qualifies` can't see from a rated band.
- **Transmit power through the premises** (§13): where the return band reaches the tuner band, 54 to 108 MHz [TR-MIDSPLIT §6.2], a
  modem's highest transmit per 6.4 MHz, less the path to its splitter leg, the splitter's isolation and the path down to a set-top or
  television on another leg, against **21 dBmV per 6.4 MHz** at the tuner: 46 dBmV is safe through the best measured in-home
  isolation of 25 dB and about 30 through the worst of 9 [TR-MIDSPLIT §6.2.1], both 21 at the tuner (our arithmetic). Reading the
  modem's highest transmit, at the band's top, against the tuner band's bottom, where it sends less, is approx and errs toward flagging.
  On the as-built fixture's high-split profile, home 2's modem reaches its set-top at about 25.5 dBmV through 22 dB of isolation.

## 20. Damage events

Task 19's events, each a cause that sets all its effects at once (invariant 5). Nothing here was checked against a real event: no
public telemetry from a car into a tap or water in a node was found, so a pass means the sim follows this section, not that it
matches a field capture.

**When.** An event starts at its onset's start (t = 0 for `Instant`) and, with a `repair_s`, ends that long after; it's at full
severity throughout, except water in a device, which ramps with its onset like water in a span (§8). A ramp of no length is a step at its
start, as `Instant` is at 0, so an event at any world time T is `Onset::Ramp { start: T, end: T }` (task 26), and its `repair_s`
counts from T.

### 20.1 A cut

Nothing crosses the cut, RF or AC: the walk adds `CUT_LOSS_DB` (300 dB, approx: a number, since it adds dB) and the AC feed
treats the span as open (§6.2), so every active past it loses power and every modem past it the signal; those fall through §7.4 as
any modem with no downstream does, T4 after about 30 s. The live side ends in a full reflection, return loss 0 dB: in phase for an
open (the conductors parted, Γ = +1), inverted for a short (center against shield, Γ = −1). An echo between two reflectors carries
the product of their signs, so at one point an open and a short give the same delay and level (§5.1) and opposite sign. A shorted
end is also a short across the hardline (§20.4), at `CUT_SHORT_OHM` (0.1 Ω, approx). The live side isn't always spared: a cut a
few dozen feet past a tap reflects back into the tap's own ports through its isolation, `isolation − tap value` below main (§5.1),
which can take that tap's modems out as well; an amp is directional, so nothing reflects back past it.

**Oracle.** 100 ft of `.500` (v_f 0.87) from a tap's through port (RL 18 dB) to an open cut: τ = 200/(0.87·983.6) = **0.2337 µs**;
at 750 MHz E = −(0 + 18 + 2·1.98) = **−21.96 dB**. Shorted, the same τ and E with the echo's sign reversed, so the response with the
open and the response with the short average to the response with neither.

### 20.2 An analog node

`Power`: the node stops, RF both ways, and with it everything below. `RfSection`: it stays powered and reports, with no forward out
and no return in. A laser that fails is task 22's `TransmitterDrift` (§16), and a fiber cut task 22's `FiberCut`; the RPD's faults
are its own (§8).

### 20.3 Water inside a device

At the device's input (a node, fed by fiber, has none, so at each of its RF outputs), a loss `k·√(f/1000)` dB, `k` its loss at 1 GHz, raised by rain as §8's span water is,
`k_eff = k·(1 + c·rain)`, and a mismatch. After `failed_after_s` it fails open (§20.1), and an active stops. A water-soaked tap that
loses nearly 30 dB at 750 MHz [PNM-HFC §7.1.6.4] [field] (impairments.md) is **k = 30/√0.75 = 34.64 dB**.

### 20.4 A short across the hardline

A resistance `R` from center conductor to shield. For RF it's a shunt across a matched line:

    T = 2R / (2R + Z0),   Γ = −Z0 / (2R + Z0),   Z0 = 75 Ω

so it loses −20·log T and reflects inverted at −20·log |Γ|. For AC, the supply that reaches it, along its power path (§6.2), drives

    I_short = V_supply / (R_path + R)

and trips if that and its load pass its rating, after which everything it feeds stops (approx: a current-limited supply folds
back; a fuse rated lower, such as an inserter port's 15 A [SCTE 265] (design-practice.md §3), isn't read yet). Below the rating,
the short's own drop on shared spans isn't modelled (approx).

**Oracles.** R = 0.5 Ω: T = 1/76, a loss of **37.62 dB**, and Γ = −75/76, a return loss of **0.115 dB**. A 90 V supply rated 15 A,
1,000 ft of `.500` at 1.7 Ω/kft to the short: I = 90/(1.7 + 0.5) = **40.9 A**, so it trips; at R = 100 Ω, I = 0.88 A, and it holds.

### 20.5 A damaged device

The car-into-tap case where the through path survives: at the device's input, a through loss, a mismatch, upstream ingress across
the return band and corrosion's CPD (§8, third order 10 dB under the second, [inferred]), and each named port cut open (§20.1). Each
effect is the existing fault at the same point, so a damaged device with one effect is that fault, to the byte. How ingress moves
with return loss in one blow has no source; the sampler draws them jointly and says so [inferred].

### 20.6 A utility outage

One event over an area, a centre and a radius: every supply inside it loses its AC input, and every home inside it its house power,
so its modems go silent (§20.1's loss at the modem). A supply with batteries carries its load for

    T = strings · per_string · V_nominal · C_Ah · η / P_load,   η = 0.8 (approx)

then stops; one without stops at once. The string's capacity is taken whole to its end-of-discharge voltage, at constant power
(approx: no Peukert). SCTE 183's "about 90 minutes" for a charged three-battery string [guideline]
fits a load near 1.9 kW (plant-and-powering.md §12).

**Oracle.** Three 12 V, 100 Ah batteries in one string, an 800 W load: T = 3·12·100·0.8/800 = **3.6 h** (12,960 s).
