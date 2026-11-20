# Upstream PNM on DOCSIS 3.0 and 3.1

What upstream power, pre-equalization, RxMER, spectrum, FEC and noise measurements mean, how they are computed, and what they say about the plant.

Tags follow the README. Source frequencies in Hz and lengths in metres are converted to MHz and feet.

## 1. Transmit and receive power

### 1.1 Terms and reference bandwidth

| Term | Meaning | Cite |
|---|---|---|
| `P1.6r_n` | Reported power of channel n: fully granted power normalized to 1.6 MHz | [PHY §7.4.12] [normative] |
| `P1.6c_n` | Commanded power, TLV-17 in RNG-RSP | [PHY §7.4.12.3] [normative] |
| `Pmax` | Max total power; default and minimum 65 dBmV, CM may advertise more | [PHY §7.4.12.2.1] [normative] |
| `Neq` | `BW_legacy/1.6 + ceil(BW_OFDMA/1.6)`, MHz | [PHY §7.4.12.3.1] [normative] |
| `P1.6hi` | `Pmax − 10·log10(Neq)`; one value for the whole TCS | [PHY §7.4.12.2] [normative] |
| `P1.6low_n` | 17 dBmV (`P1.6min`), plus 1 dB (50 kHz) or 0.5 dB (25 kHz) with boosted pilots | [PHY §7.4.12.2] [normative] |
| `P1.6load_n` | `P1.6hi − P1.6r_n`; the lowest is the highest loaded channel, `P1.6load_1` | [PHY §7.4.12.2.1] [normative] |
| `P1.6load_min_set` | Commanded dB below `P1.6hi` marking the top of the DRW | [PHY §7.4.12.3.1] [normative] |
| TCP | Total composite power: linear sum of channel powers in the TCS | [PNM-HFC §8.14.1] [guideline] |

- OFDMA transmit is always reported per 1.6 MHz (32 subcarriers at 50 kHz, 64 at 25 kHz). With boosted pilots, reported power is 1 or 0.5 dB above the average RF power of that probe [PNM-HFC §6.1.2.1] [PHY §7.4.12.2.1] [normative].
- SC-QAM is reported per 1.6 MHz when CM and CMTS are both D3.1, and per occupied bandwidth if either is D3.0 [PNM-HFC §6.1.2.1] [guideline]. A 6.4 MHz channel carries `P1.6r_n + 6 dB` [PHY §7.4.12.2] [normative], so a D3.1 CM replacing a D3.0 CM reads 6 dB lower with no plant change [PNM-HFC §6.1.2.1] [guideline].
- Actual channel power is `P1.6r_n + 10·log10(BW_MHz/1.6)` [PNM-HFC §8.14.1] [guideline].
- The CMTS input expects a 6.4 MHz equivalent, typically 0 dBmV; SC-QAM and OFDMA of equal width look the same in PSD [PNM-HFC §6.1.2.4] [guideline].

Per-subcarrier target with pre-eq on, where `P1.6delta_n` is 0, 0.5 (25 kHz boosted) or 1 dB (50 kHz boosted); boosted pilots add `10·log10(3)`, probes add `Probedelta_n`, and pre-eq off drops `PreEq_i` [PHY §7.4.12.3] [normative]:

    Pt_sc_i = P1.6r_n − P1.6delta_n + PreEq_i(dB) − 10·log10(32 or 64)

### 1.2 Limits

- D3.1: 65 dBmV total, at least 53.2 dBmV/1.6 MHz. `65 − 10·log10(15) = 53.2`, so full power needs at least 24 MHz of modulated spectrum [PNM-HFC §6.1.2.2] [guideline]. A D3.1 CMTS limits commanded `P1.6hi` to `53.2 + (Pmax − 65)` when modulated spectrum is ≤ 24 MHz; not with a D3.0 CMTS [PHY §7.4.12.2] [normative].
- The per-1.6 MHz ceiling falls as the TCS widens, through `Neq` [PHY §7.4.12.2] [normative]. Values at `Pmax` 65 [inferred from the formula; 47 and 95 MHz match PNM-HFC §6.1.2.2 and §6.1.2.1]:

| OFDMA spectrum (MHz) | 24 | 47 | 80 | 96 | 192 |
|---|---|---|---|---|---|
| `Neq` | 15 | 30 | 50 | 60 | 120 |
| `P1.6hi` (dBmV/1.6 MHz) | 53.24 | 50.23 | 48.01 | 47.22 | 44.21 |

- Usable bandwidth may exclude guard bands: 48 MHz with 0.5 MHz guards is 47 MHz [PNM-HFC §6.1.2.2] [guideline].
- Accuracy: actual within ±2 dB of `P1.6r_n` with pre-eq off [PHY §7.4.12.2.1] [normative]. Before fine ranging, up to 9 dB under `P1.6low_n` per subcarrier [PHY §7.4.12.2] [normative].
- D3.0, assuming ≥ 32-QAM TDMA in all channels: TCP capability 57 dBmV for one to four channels; per-channel limit `Phi_1` 57, 54, 51 dBmV for 1, 2, 3 to 4 channels [PNM-HFC §8.14.1, §8.14.2] [guideline].
- Maximum Scheduled Minislots lets a CMTS command PSD above what a full grant allows by limiting concurrent minislots, with graceful fidelity loss [PHY §7.4.12.1] [normative].

### 1.3 Reported objects

All per [PNM-HFC §6.1.3] [normative text quoted there]:

- `docsIf31CmUsOfdmaChanTxPower`: `P1.6r_n` in quarter dBmV, 68 to `213 + 4·(Pmax − 65)`, that is 17 to 53.2 dBmV at `Pmax` 65.
- `docsIf31CmUsScQamChanTxPsd`: `P1.6r_n` of SC-QAM. `docsIf3CmStatusUsTxPower`: SC-QAM total average power (`Pr`) on any CMTS. So the §1.1 rule describes `P1.6r_n`, while the D3.0-style MIB stays total power [inferred].
- `docsIf31CmtsCmUsOfdmaChannelRxPower`: per-CM OFDMA receive, per 1.6 MHz at CMTS input. `docsIf31CmtsUsOfdmaChanTargetRxPower`: expected receive PSD per 1.6 MHz. `docsIf3CmtsCmUsStatusRxPower`: total average SC-QAM receive. `docsIf3CmtsSignalQualityExtExpectedRxSignalPower`: expected receive.

PNM test `UsOfdmaRxPower`: average power per probe subcarrier plus `10·log10(32)` or `10·log10(64)`, plus 1 or 0.5 dB if boosted; `RxPowerOnePtSixPsd` in tenth dB; pre-eq selectable, default on [CCAP-OSSI §7.3.5.4] [normative]. Averaging at least 1 to 32 probes; standard deviation ≤ 0.33 dB with 8-probe averages at 25 dB CNR within ±6 dB of 0 dBmV [PHY §9.4.7] [normative].

### 1.4 Dynamic range window

D3.0: maximum power difference across TCS channels. D3.1: maximum difference in power per 1.6 MHz [PHY §3] [normative]. Normally 12 dB, enough for 10 dB of band-edge to band-edge tilt [PHY §7.4.12.2] [normative]:

    P1.6low_multi = P1.6hi − P1.6load_min_set − 12
    P1.6load_min_set ≤ P1.6hi − P1.6r_n ≤ P1.6load_min_set + 12

A CM commanded outside flags an error bit in the RNG-REQ SID field until cleared, and fidelity requirements stop applying [PHY §7.4.12.2] [normative]. Pre-eq variation across a channel does not count against the DRW; initial ranging defaults to one third up from the lower channel edge [PNM-HFC §6.1.2.3] [guideline].

### 1.5 Transmit headroom

    TCP headroom    = Pmax − commanded TCP              (D3.1);  57 − TCP (D3.0, ≥ 32-QAM)
    Min Ch headroom = P1.6hi − max(P1.6r_n) = P1.6load_1 (D3.1);  Phi_1 − Pr_1 (D3.0, ≥ 32-QAM)
    Neq_additional  = (10^(MinChHeadroom/10) − 1) · Neq

Per [PNM-HFC §8.14.1 to §8.14.3] [guideline]. With tilt, Min Ch runs out first. Exhausted by: TCP over `Pmax` (channels dropped into partial service); wider TCS; higher CMTS set point; DRW violation (most loaded channels eventually dropped); any net attenuation change through long-loop ALC; uneven loss compensated by pre-eq. Results: FEC errors, lower bit loading, partial service.

## 2. Echo types

| Type | Path | Channel response | Equalizer response | Cite |
|---|---|---|---|---|
| Multiple recursion (IIR) | Two reflectors on the path | Decaying series, spacing `2·t`, ratio Γ1Γ2 | One tap after main | [PNM-HFC §6.2.1.1.1] [PNMP §V.3] |
| Single recursion (FIR) | Leak through poor tap isolation, reflects beyond the tap | One echo | Decaying series to end of filter, alternating signs | [PNM-HFC §6.2.1.1.2] [PNMP §V.5.3] |
| Composite | Poor amp return loss plus open port; two delays unless co-located | Two echoes | Both | [PNM-HFC §6.2.1.1.3] |
| Reflection to a matched source | One reflection, no re-reflection | Flat, lower level | Nothing useful; needs field standing-wave test | [PNM-HFC §6.2.1.1.4] |

All [guideline] unless marked:

- IIR delay depends only on reflector spacing, wherever the signal starts. It gives cavity length, not position on the plant [PNM-HFC §6.2.1.1.1].
- A CM can be one reflector: a 3 dB pad at the CM port drops the echo 6 dB or more if so [PNM-HFC §6.2.1.1.1].
- In the FIR case the reflector is not on the modem's path to the CMTS; it lies beyond the modem's tap. Strength tracks isolation minus tap value: a 23 dB tap at 38 dB isolation leaks 15 dB below main; an 11 dB tap is 12 dB better at equal isolation. So it shows most on high-value taps [PNM-HFC §6.2.1.1.2]. Field echoes of this kind are often 30 dB or more down and numerous on a span [PNM-HFC §6.2.1.1.3] [field].
- Fixing either defect of a cavity removes the echo, but the other defect stays [PNM-HFC §6.2.1.1.4].
- Ripple: `20·log10((1+|Γ_E|)/(1−|Γ_E|))` dB [PNMP App IV]. −18 dBc → 2.2 dB, −25 dBc → 1 dB [PNM-HFC §8.11]; −16 dB at 0.3125 µs → 2.78 dB p-p [PHY §7.4.17]. Cavity: `492·v_f / ripple_spacing_MHz` ft [PNM-HFC §6.2.5.3.1]; 234 ns, 4.27 MHz, v_f 0.87 → about 100 ft [PNMP §I.6].

Group delay, `GD = −dφ/dω`, comes from AC chokes (5 to 10 MHz), diplex filters near cutoff, roll-off, HPFs, in-line equalizers, and reflections [PNM-HFC §6.2.1, §I.4] [guideline]. Up to 300 ns in a band-edge channel; Figure 139 shows it rising with cascade (node, +1, +2, +3 actives), steeply above 0.9 of the band edge [PNM-HFC §6.2.6.3.12] [field]. A 1996 model limited the quadratic part to 10 ns p-p per MHz below 35 MHz and 20 ns p-p per MHz for 35 to 45 MHz, aimed at low-order SC-QAM [PNM-HFC §I.4] [guideline].

## 3. SC-QAM pre-equalizer

Filter: 24 complex coefficients F1 to F24, T = 1/R_sym spacing [PNM-HFC §6.2.3] [guideline]. CMTS measures RNG-REQ and replies with 24 coefficients and a set (replace) or adjust (convolve) flag. Adjust is the norm, and a satisfied CMTS sends main tap only. Convergence takes a few iterations. Moderate distortion is fully compensated with no FEC symptom, which is what buys proactive time. CM coefficients show current predistortion, approximately the plant inverse; CMTS coefficients show what remains, and settle flat [PNM-HFC §6.2.2, §6.2.3, §6.2.3.2] [guideline].

| Item | Value | Cite |
|---|---|---|
| Main tap | Usually 8; some CMTSs as low as 6; D1.1 uses 4 | [PNM-HFC §6.2.6.3.1] [guideline] |
| Taps per symbol | 1 on D2.0 and D3.0 | [PNM-HFC §6.2.3.4] [guideline] |
| T | 0.1953, 0.391, 0.781 µs at 5.12, 2.56, 1.28 Msym/s | [PNM-HFC Table 9] |
| Reach after main | 16T: 3.125, 6.25, 12.5 µs; total span 23T = 4.49 µs at 5.12 | [PNM-HFC Table 9, §6.2.5.1] |
| Bandwidth seen | Symbol rate: 5.12 of 6.4 MHz, 2.56 of 3.2, 1.28 of 1.6 | [SCTE 297 §6.4] [guideline] |
| Resolution | `1/(NT)` = 213.33 kHz at 5.12 Msym/s | [SCTE 297 §1.3] [guideline] |

Reach is an upper bound: near the limit no later taps refine the fit, off-grid echoes need more taps, and strong echoes need taps at 2 or 3 times their delay; 64-QAM suffers first [PNM-HFC §6.2.3.4] [guideline].

Format: `docsIfCmStatusEqualizationData`, `docsIfCmtsCmStatusEqualizationData` (D2.0); `docsIf3CmStatusUsEqData`, `docsIf3CmtsCmUsStatusEqData` (D3.0, per channel). Header bytes main tap, taps per symbol, tap count, reserved (`08 01 18 00`), then 24 pairs of 16-bit I and Q, 100 bytes [PNM-HFC §6.2.3.3, §6.2.6.4.1, Table 22] [guideline]. CMs vary: max amplitude 2047, 1023 or 511; four-nibble (spec) or three-nibble two's complement; dropping the first nibble decodes both [PNM-HFC §6.2.6.2] [guideline]. That is for CMs: the example CMTS string has main `3F 20` = 16160 [inferred from PNM-HFC §6.2.6.4.1]. `docsIfSigQEqualizationData` is different data, a downstream blind equalizer, often unreliable [PNM-HFC §6.2.3.3.1.2] [guideline].

## 4. SC-QAM metrics

From [PNM-HFC §6.2.6.3], identical in [PNMP §6.3] [guideline], with main tap m:

    MTE = |F_m|²    PreMTE = Σ_{k<m}|F_k|²    PostMTE = Σ_{k>m}|F_k|²    TTE = PreMTE + MTE + PostMTE
    MTC       = 10·log10(TTE / MTE)
    MTR       = 10·log10(MTE / (PreMTE + PostMTE))
    NMTER     = 10·log10((PreMTE + PostMTE) / TTE)
    PreMTTER  = 10·log10(PreMTE / TTE)        PostMTTER = 10·log10(PostMTE / TTE)
    PPESR     = 10·log10(PreMTE / PostMTE)    PPTSR     = 10·log10(|F_(m−1)|² / |F_(m+1)|²)

Exact relations [inferred]: `MTR = −NMTER − MTC`; `MTC = −10·log10(1 − 10^(NMTER/10))`. Table 12's N+5 row checks: NMTER −15.9 gives MTC 0.11.

Meaning, all [guideline]:

- MTC above 2 dB: compensation may no longer succeed; 2 dB of MTC means 2 dB less at the CMTS; any MTC in the CMTS set should alarm [PNM-HFC §6.2.6.3.6]. That point is NMTER −4.33, MTR 2.33 dB [inferred].
- NMTER: first-pass screen and an RxMER estimate if the impairment is purely linear [PNM-HFC §6.2.6.3.8]. MTR: nearly the same size except at extreme distortion [PNM-HFC §6.2.6.3.7].
- PreMTTER with symmetry shows group delay; PostMTTER shows reflection. PPESR is positive when group delay dominates, negative when reflection does [PNM-HFC §6.2.6.3.9 to §6.2.6.3.12.2]. Reflection level is MTR over taps 9 to 24, group delay level over taps 1 to 7 [PNM-HFC §8.11].

## 5. How echoes and group delay land on taps

An off-grid delay spreads over neighbouring taps, with some pre-main energy, more for short echoes [PNM-HFC §6.2.3.4, §6.2.6.3.12.3] [guideline]. Most in-home echoes are under 164 ft (50 m) and affect only taps 9 and 10 [PNM-HFC §8.11.6] [guideline]. At v_f 0.87, 1T at 5.12 Msym/s is an 83.6 ft cavity [inferred].

Fractional delay from three taps around the peak, in dB: `a = (y0 − 2·y1 + y2)/2`, `x_out = x1 + (y0 − y2)/(4·a)`, `y_out = y1 − (y0 − y2)²/(16·a)`. Taps 9, 10, 11 at 35, 40, 29 dB give 9.8125 and 40.28 dB [PNM-HFC §8.10.4] [guideline].

Band-edge group delay raises pre-main taps with cascade while MTC stays low; a combined case is roughly the sum of the separate ones [PNM-HFC §6.2.6.3.12.1, §6.2.6.3.12.3] [guideline]. Lab data, 3.2 MHz channel [PNM-HFC Tables 11 to 16] [field], with Table 11 read from the PDF:

| Case | Cascade | MTC | NMTER | PreMTTER | PostMTTER | PPESR | Main, tap 7, tap 9 (dB) |
|---|---|---|---|---|---|---|---|
| 40.4 MHz, no echo | N+0 | 0.01 | −29 | −29.4 | −39.4 | +9.93 | −0.01, −30.2, −47.2 |
| 40.4 MHz, no echo | N+5 | 0.11 | −15.9 | −16.3 | −25.9 | +9.53 | −0.11, −17.1, −26.1 |
| 40.4 MHz, 0.5 µs echo | N+5 | 0.48 | −9.84 | −15.8 | −11.1 | −4.65 | −0.48, −16.5, −14.4 |
| 14 MHz, 0.5 µs echo | N+5 | 0.80 | −7.73 | −17.4 | −8.23 | −9.18 | −0.8, −18.1, −11.9 |

## 6. Thresholds and severity

Several schemes, none binding; tune for cascade, node size and utilization [PNM-HFC §8.11] [guideline].

| Scheme | Green | Yellow | Red | Cite |
|---|---|---|---|---|
| NMTER, 64-QAM | below −30 dB | −30 to −27 dB | −27 dB and above | [PNM-HFC §6.2.6.3.8] [PNMP §6.3.8] |
| MTR, new to PNM | ≥ 22 dB | 11 to 22 dB | < 11 dB | [PNM-HFC Table 26] |
| MTR, established, under 4 amps | ≥ 25 dB | 18 to 25 dB | < 18 dB | [PNM-HFC Table 27] |
| MTR, lower modulation than expected | ≥ 32 dB | 25 to 32 dB | < 25 dB | [PNM-HFC Table 28] |
| Distortion or reflection level | ≤ −25 dBc | −25 to −18 dBc | ≥ −18 dBc | [PNM-HFC §8.11.2] [PNMP §6.7.2] |
| MTC at the CM | | | > 2 dB | [PNM-HFC §6.2.6.3.6] |

NMTER basis: 27 dB CNR for negligible errors at 64-QAM, 30 dB where correctables begin [PNM-HFC §6.2.6.3.8].

Over time, for yellow modems [PNM-HFC §8.11, §8.11.3, §8.11.4] [guideline]: 3 dB of change is worth attention, compared at the same time of day. Trending red: `MR_last + 3·D > −18` with D in dB per day; `days_to_repair = (−18 − MR_last)/D`. Intermittent red: `avg_4h_|Δ| / (−18 − avg_level) > 0.25`; near 1 swings reach the threshold, below 0.1 is safe. Plant health: `10·(1 − (red + 0.5·yellow)/(total − non_responding))`, or with `total + non_responding` if green modems dropped for outside reasons; a healthy system sits near 7, computed per upstream rather than node (formulas from the PDF). Polling: daily rotating eight-hour shifts, 4 hours for flagged CMs (kept 48 hours after clearing), 10 minutes live [PNM-HFC §6.2.6.4.2] [guideline].

## 7. Frequency response from coefficients

SC-QAM: place F1..F24 at inputs 9 to 32 of a 32-point FFT, 1 to 8 zero, so tap 8 is input 16; zero-pad for more points. Result is the pre-equalizer response over `fc ± R_sym/2`; invert for the channel. Calibrate per bin: `|F'| = |F|/|F_C|`, `φ' = φ − φ_C`, using a short reference plant per CM and CMTS model pair [PNM-HFC §6.2.6.6.1, §6.2.6.6.2] [guideline]. Short plants are not ideal: Table 22's short-plant string gives MTR about 21.3 dB [inferred from the string].

Across channels [SCTE 297 §1.3, §1.5, §6.2, §6.7, §6.9] [guideline]:

- Coefficient response is approximately the plant inverse, so a display needs normal and inverse modes. Stitch per-channel responses; vacant spectrum shows nothing. Show CM transmit and CMTS receive power too, because the former is capped and long-loop ALC holds the latter.
- Updates lag: 20 s station maintenance on four channels is 80 s; corrections over 3 dB may take several cycles. Usable only where the station maintenance profile demodulates. A channel in partial mode stops showing its impairment.
- Lab: a 28.1 MHz notch read about 38 dB on a VNA, sat off frequency near 26 MHz on sweep, and about right on pre-eq traces [field].

RxMER thresholds [SCTE 297 Table 2] [guideline]: SC-QAM QPSK 7 to 10, 16-QAM 15 to 18, 64-QAM 22 to 24 dB. OFDMA QPSK 11, 8-QAM 14, 16-QAM 17, 32-QAM 20, 64-QAM 23, 128-QAM 26, 256-QAM 29, 512-QAM 32.5, 1024-QAM 35.5, 2048-QAM 39, 4096-QAM 43 dB. Below that, partial mode is likely.

## 8. Grouping and localization from pre-eq

Signature: a level and delay pair [PNM-HFC §8.10.2] [guideline]. Logic [PNM-HFC §8.10, §8.12.1, §8.8.9.3] [guideline]:

- Fault lies on the path shared by modems with the signature, excluding the path shared with clean ones. More modems per branch sharpens it. Sides of a street may be on different legs.
- One device in a home: in-home. One subscriber, not another on the tap: drop or in-home. Several on a tap or leg: at or above the tap, cable or active.
- A delay matching a known span narrows the cause. Distance is between two reflectors, so an outside fault can be tagged in-home by mistake.
- Modems can see one fault at different delays: in case A, CM1 and CM2 near the reflecting tap read less than CM3 (2.8T), which has an extra transit [PNM-HFC §8.10.3] [field].

Matching [PNM-HFC §8.11.6.1] [guideline]: same logical channel only; optionally drop unimpaired modems (MTR above about 25 dB); compare all pairs; threshold; force a symmetric matrix; form groups; reject strays with GIS; line tickets for groups, service tickets for loners. Too low a threshold groups same-factory modems.

1. Frequency division: `Q(f) = F_A(f)/F_B(f)`, IFFT, compute MTR. Threshold 18 dB in the matrix example, 25 dB in the method text. Sample: MTR 14.09 and 15.3 dB modems, quotient 27.3 dB, an excellent match [PNM-HFC §8.11.6.2] [field].
2. Time subtraction: normalize main to unity, subtract, restore main (1, or `√(1 − Σ other energy)`), compute MTR [PNM-HFC §8.11.6.3] [guideline].

Zeroing close-in bins removes in-home echoes that would break a shared-echo match, if the plant echo is long [PNM-HFC §8.11.6.2] [guideline]. Raw phase must be corrected before complex comparison [PNM-HFC §9.5.5] [guideline]. Bad states: no solution because of deep suckout or long echo; impulse noise upsetting adaptation; bad MIB data; pre-eq off [PNM-HFC §8.12] [guideline].

## 9. OFDMA pre-equalizer

Mechanism [PHY §7.4.17] [normative]: one complex coefficient per subcarrier. Default `1 + j0` on used and unused subcarriers, zero on excluded. Set replaces; adjust multiplies, `C_k(i+1) = C_k(i)·A_k(i)`. **The CM then normalizes so `mean(|C_k|²) = 1`** over used and unused subcarriers. All non-probe bursts are pre-equalized, and probes unless P-MAP disables it.

| Set | Name | Format | Value 1 | File type | Meaning |
|---|---|---|---|---|---|
| CM | `PNMUsPreEq` | s2.13 | `0x2000` | `504E4E06` | In use, approximately the plant inverse |
| Last update | `PNMUsPreEqLastUpdate` | s1.14 | `0x4000` | `504E4E07` | Last Adjust or Set from CMTS; residual channel |

Both are 16 bits each for I and Q, stored in the CM, and clip on I and Q independently, so s2.13 caps a component just under ±4 and s1.14 under ±2. Files cover the encompassed spectrum, with subcarrier-zero frequency, `FirstActiveSubcarrierIndex`, spacing and length [CM-OSSI §D.2.13] [normative]. 96 MHz at 50 kHz is 1,920 coefficients [PNM-HFC §6.2.5.1].

`PreEqCoAdjStatus` [CM-OSSI §D.2.13] [normative]: `other(1)` modified other than clipping; `success(2)`; `clipped(3)`, for example excess tilt or ripple; `rejected(4)`.

Interpretation [PNM-HFC §6.2.5.2] [PNM-3.1 §6.2.1] [guideline]:

- Coefficients reflect the plant fully if the impulse response fits inside the cyclic prefix. Deviation comes from NDI during estimation, echo beyond the prefix, or change since the last update.
- Updates near unity: stable. Far from unity: instability. Current values far from 0 dB: CM stressed.
- Large current versus last difference, on several CMs or repeatedly: unstable plant; can reflect NDI, wind, vibration, moisture [PNM-HFC §8.5.2].
- Any status but success justifies maintenance: one CM alone points at that CM, a group at the plant.
- Burst-receiver equalization is invisible; inverting the CM set is a sufficient channel estimate almost always.

Summary attributes are computed on query and not stored in files [CM-OSSI §D.2.13] [normative], with uses from [PNM-HFC §6.2.5.3] [guideline]. Units 0.001 dB or 0.001 ns, slopes per MHz:

- `AmplitudeRipplePkToPk`: ripple, and its spacing gives cavity length. `AmplitudeRippleRMS`: reflection size, correct for cable loss if distance is known.
- `AmplitudeSlope`: in-channel tilt, removed before ripple. `AmplitudeMean`: mean coefficient magnitude.
- `GroupDelayRipplePkToPk`, `GroupDelayRippleRms`: compare with amplitude ripple to spot look-alikes; slope is subtracted first. `GroupDelaySlope`: channel near filter cascade band edge, long ranging. `GroupDelayMean`.

Fit algorithm: [CM-OSSI §D.4] and `docs/physics.md` §5.5.

Processing [PNM-HFC §9.4.1 to §9.4.3] [guideline]: raw data carries an arbitrary linear delay, so phase rotates with frequency. Unwrap, remove the delay, rotate the t = 0 term real, before comparing phase across modems. Magnitude needs none of this. Time-domain spacing from main survives the shift.

Time and distance [PNM-HFC §6.2.5.1, §9.5.4] [guideline]: round-trip resolution `1/BW`, 10.417 ns at 96 MHz; span `1/Δf`, 20 µs at 50 kHz. Upstream FFT rate 102.4 MHz, 2048 at 50 kHz, 4096 at 25 kHz. Length per bin `L' = (c·v_f/(2·R_s))·K`, `K = R_s/BW_active`; 22 MHz active at v_f 0.85 gives `4.0822 × 4.6545 = 19.0 ft`. Tilt becomes a decaying time tail and in-band ingress smears across time, so peaks can mislead [PNM-HFC §9.5.1] [guideline].

## 10. Upstream OFDMA RxMER

Per subcarrier, from probes, for one specified CM: reference power over mean error-vector power, after equalization. Those probes are usually not the pre-eq ones [PHY §9.4.8] [CCAP-OSSI §7.3.5.5] [normative].

`UsOfdmaRxMerPerSubcarrier` [CCAP-OSSI §7.3.5.5] [normative]: `CmMac`, `PreEqOnOff` (default true), `NumAverages` 1 to 255 with leaky integrator `α = 1 − 1/N`. File type `504E4E69`. Data [CCAP-OSSI §7.3.2.5] [normative]: one byte per subcarrier, lowest to highest active, no gaps; quarter dB, clamped 0 to 63.5 dB; `0xFF` means no measurement.

Accuracy [PHY §9.4.8] [normative]: 95 MHz, 10 dBmV per 6.4 MHz, 8-symbol probes: standard deviation ≤ 0.5 dB; a 30 to 35 dB CNR step must read 4 to 6 dB. CM transmit MER minimum: 44 dB (100% grant), 50 dB (5% grant) with optimum pre-eq; 40 dB without [PHY §7.4.12.5.3.2] [normative].

Reality [PNM-HFC §6.3.1, §9.2] [field]: `docsPnmCmtsUsOfdmaRxMer*` under `.1.3.6.1.4.1.4491.2.1.27.1.3.7.1`; often one modem per channel at a time; can block other PNM tests or disturb service; hourly wanted, rarely possible.

Analysis [PNM-HFC §6.3.4] [guideline]:

- A single capture finds static faults, repeats find trends, spurs need many. Several CMs at the same time and frequency point to common plant.
- Per-subcarrier history as percentiles: large 1-to-5% gap with small gaps elsewhere means intermittent; even, wide spacing means persistent.
- High variance with skewness near zero: persistent. High variance with large absolute, especially negative, skewness: intermittent; high excess kurtosis reinforces it.
- Averaging to 1 MHz (20 subcarriers at 50 kHz) gives a cheap time series.

Field [PNM-HFC §6.3.4.3, §6.3.4.8] [field]: up to 20 dB spread on one subcarrier over time; daily cycle 5 to 10 dB; intermittent 18 dB drop over 4 MHz; 10 dB channel-wide steps. An interferer under active subcarriers can be invisible on UTSC yet drop RxMER [PNM-HFC §8.6.5.1.3] [guideline]. Impulse noise degrades codewords while RxMER looks normal [PNM-HFC §7.2.1.1] [guideline].

## 11. Upstream triggered spectrum capture

Requirements [PHY §9.4.2] [normative]: full band, 100 kHz or finer bins. 3.1 mode triggers: free running, timestamp, MAC address, idle SID, active or quiet probe symbol. Pre-3.1 mode: free running, minislot count, SID, idle SID. Config also has burst IUC [CCAP-OSSI §7.3.5.6]. Only free running is universal; idle SID, which shows the floor under carriers, is limited [PNM-HFC §8.8.5] [field]. All known deployments need proprietary elements [PNM-HFC §9.2] [field].

Config [CCAP-OSSI §7.3.5.6] [normative]: `CenterFreq`, `Span`, `NumBins`, `Averaging` (0, 2 to 255), `Window` (default rectangular), `RepeatPeriod` (default 100,000 µs), `FreeRunDuration` (default 1,000 ms), `TriggerCount`. `OutputFormat`: `fftPower` default, `timeIQ` mandatory; `fftAmplitude`, `fftIQ`, `rawAdc`, `fftDb` (hundredths dBmV) optional. File type `504E4E6A`, with `CalibrationConstantK` (hundredths dB) for bins to dBmV.

Rates [PNM-HFC §6.4.3, §6.4.4, Table 23] [field]: proprietary SNMP 2 to 10 s, typically 150 kHz. TFTP UTSC ~1 Mbps, 200 ms to 1 s, 50 kHz sub and mid-split, 100 kHz high-split. RPD over L2TP 80 to 100 Mbps, 100 to 200 ms; example 50 kHz at 50 ms.

Signatures [PNM-HFC §6.4.2, §7.2.1, §8.8.5] [guideline], detail in `impairments.md`: impulse or burst, about 1 µs to tens of µs; narrowband (CB, ham, shortwave, FM) low impact but flags a shield break; switching regulator comb, for example every 50 kHz; common mode disturbance, 150 kHz to 30 MHz, hump near 21 to 23 MHz; CPD, second order resembling downstream SC-QAM, third order raising the floor; HPNA, wideband and severe. Field: burst ingress at 2 to 16 MHz with lowest channel SNR 29.2 dB [PNM-HFC §8.8.5] [field].

## 12. Upstream FEC statistics

Per data IUC, channel and CM, 64-bit [PHY §9.4.5] [normative]: pre-FEC error-free (passed pre-decode syndrome), corrected (failed pre, passed post), unreliable (failed post; pessimistic), and total, expected to be the sum. Per CM: MAC CRC failures, total MAC frames. Summaries on enable: 10 minute (errored seconds, 600 one-second counts) and 24 hour (errored minutes, 1440 counts), with parallel MAC CRC sets. The guides list only the 10 minute one, per channel [PNM-HFC §6.6.2] [PNM-3.1 §6.6.2]. Legacy per-CM `Unerroreds`, `Correcteds`, `Uncorrectables` are Counter32 in `CmtsCmUsStatus` [CCAP-OSSI §7.2.2.2.2] [normative].

Patterns [PNM-HFC §6.6.4] [guideline]: correctables in every interval suggest arcing on utility power; uncorrectables in consecutive seconds, another data transmission; long error runs split by long clean runs, two-way radio. Not fielded [PNM-HFC §6.6.6].

## 13. Impulse noise, histogram and probes

**Impulse noise** [PHY §9.4.3] [CCAP-OSSI §7.3.5.2] [normative]: band-pass in an unoccupied band, up to 5.12 MHz. Above threshold starts an event, below ends it; threshold zero measures average band power. Records timestamp (98 ns resolution, about 7 minute range), duration and average power; ring buffer of 1024. Defaults `StartTriggerLevel` 300 µV (−10.46 dBmV), `EndTriggerLevel` 150 µV (−16.48 dBmV) (conversions ours), `CenterFrequency` 7 MHz, `MeasurementBandwidth` 2560 kHz (160 to 5120 by doubling). Proprietary field use only [PNM-HFC §6.5.4].

**Histogram** [PHY §9.4.6] [normative]: full-band ADC samples, two-sided, 256 bins even or 255 odd symmetry, 32-bit dwell and hit counts, at least 10 million samples in 30 s or less, density down to 10⁻⁶. Clipping truncates one tail and leaves a spike; in real plant the tail is reduced with a spike near +1.25 σ [PNM-HFC §6.8.2] [field]. Not developed in the field to spec [PNM-HFC §6.8.3].

**Probes** [PHY §9.4.1] [normative]: captures at least one OFDMA symbol. Active: all non-excluded subcarriers, pre-eq normally off, known input, so partial network analyzer for linear and nonlinear response. Quiet: all off, floor only. Time-domain samples of a full symbol with guard interval at 102.4 Msps; reports excluded subcarriers, cyclic prefix, roll-off, FFT start sample, timestamp, P-IE. Controls `CmMacAddress`, `UseIdleSid`, `PreEqualizationOn`, `NumSymbolsToCapture` [CCAP-OSSI §7.3.5.1] [normative]. Not field tested at publication [PNM-HFC §6.9.4].

**CM capture.** FBC can reach the upstream band, but the diplex limits it; many CMs now tap the band before the filter, called upstream data analysis (UDA). A capable CM near a source shows spikes, because taps and splitters isolate port to port. CPD has not been seen this way. One passage calls such modems uncommon [PNM-HFC §6.7.3, §8.8.2, §8.8.7] [guideline].

## 14. Noise funneling and localization

Many transmitters, one receiver: ingress anywhere raises the floor for all modems at that frequency, cutting SNR and raising codeword errors, channel by channel. Per-modem data does not generally locate the source. Bonded modems shift channels at lower capacity. Ingress in vacant spectrum flags a shield break but has no service impact [PNM-HFC §8.8.1.1, §8.8.2, §8.8.4, §8.8.10] [guideline].

- **Divide and conquer** [PNM-HFC §8.8.2, §8.8.6, §8.8.10] [guideline]: breadth-first down the logical tree; at each junction disconnect or attenuate a leg and see if ingress changes; follow that leg; then tap by tap.
- **Ingress switch** [PNM-HFC §8.8.9.1] [guideline]: per upstream input port on smart amps. On, Off (very high attenuation), Attenuated (typically fixed 6 dB). SCTE 283 `UsIngressSwitchCfg`. Legacy wink switches do similar.
- **Headroom first** [PNM-HFC §8.8.8] [guideline]: 6 dB added can force partial service, lower profile or offline; short switching can cause bit errors. Usually fine on sub- and mid-split, riskier on high-split.
- **Topology discovery** [PNM-HFC §8.8.9.3] [guideline]: switch a leg's attenuation and watch modem transmit rise. Finds the leg, not the tap.
- **UDA** [PNM-HFC §8.8.7] [guideline]: detect on UTSC (uneven max hold, raised quiet floor, energy in gaps); search UDA captures of CMs below that node for positive deviations; compare bin by bin. One match: that home. Several: their common equipment. About 15 quiet-time captures with max hold, ideally overlapping UTSC, catch bursts. Very high transmit can overload the capture [field].

Priority uses impact: spectrum plus profile, MER and FEC [PNM-HFC §8.8.7] [guideline].

## Implications for hfc-twin

All [inferred]. Section numbers are `docs/physics.md` unless marked here.

- **§5.1, §5.2, §8 echoes.** Model an echo as a cavity between two reflectors, the near-modem port being one case. A plant cavity gives the same τ to every modem below it. Add the off-path class: a reflector beyond a modem's tap reaches it through tap isolation, so "every modem whose path crosses the point" misses it. This needs tap isolation in the component spec.
- **§5.3 taps.** Generate taps as a T-spaced inverse of `H(f)` over `fc ± R_sym/2`, not one tap at `8 + round(τ/T)`, so fractional delays spread and pre-main energy appears. Add diplex and choke group delay with cascade so PreMTTER and PPESR move. Add a device floor to the modem spec, since a short plant reads MTR near 21 dB.
- **§5.3 thresholds.** Keep schemes as diagnosis config. The §5.3 oracle (NMTER −29) is yellow by NMTER and green by the MTR and dBc schemes.
- **§5.4, §5.5 OFDMA.** Coefficients are `c_k ∝ 1/H(f_k)`, normalized to `mean(|c_k|²) = 1`, so flat loss goes to `P1.6r` and not the coefficients, and `mag_mean_db` sits near 0 dB. Clip to s2.13 on I and Q, zero excluded subcarriers. Define `gd_mean_ns` with zero bulk delay in the sim and have adapters remove it from real data. Pre-eq group delay has the opposite sign to the channel's.
- **Update model** (deferred in `docs/tasks/01-schemas.md`). Status and last update need little: `A_k = c_k(t)/c_k(t−1)` in s1.14, unity on stable plant, `clipped(3)` from the s2.13 limit, with a lag of station maintenance period per channel.
- **§7.2 power.** Compute `Neq`, `P1.6hi`, TCP and both headrooms per TCS. Compare in matching units, not `P_tx` per 6.4 MHz against a limit. Add D3.0 limits and the 12 dB DRW with partial service on violation. Carry `P1.6r` and total SC-QAM power separately.
- **§7.3 funneling.** `N_node(f)` per port with per-leg attenuation supports the switch test. Localization runs on leg attenuation plus UTSC, and UDA; transmit deltas serve topology discovery. `us_rxmer` needs quarter-dB rounding, 0 to 63.5 clamp, a None state for no measurement, a residual distortion term, finite CM transmit MER, and per-CM receive power so it differs by CM over a shared floor. `us_snr_db` can stay channel-level.

§9 additions the sim can derive:

| Field | From |
|---|---|
| `preeq.main_tap_index`, `pre_mttr_db`, `post_mttr_db`, `ppesr_db`, `pptsr_db` | §4 here |
| `preeq_last_update[subcarrier]`, `preeq_status` | §9 here |
| `us_rx_dbmv_per_1p6mhz[channel]` per CM | §7.1, ranging |
| `us_total_tx_dbmv`, `tcp_headroom_db`, `min_ch_headroom_db` | §1 here |
| `us_codewords{unerrored, corrected, unreliable}[channel][iuc]`, `us_mac_crc{failed, total}`, errored seconds | CMTS side of §4.6 |
| `utsc_dbmv[bin]`, quiet and with traffic, per port | `N_node(f)` plus carriers |
| `active_probe`, `quiet_probe` | `H(f)` with pre-eq off; `N_node(f)` |
| `uda_fbc_dbmv[bin]` for capable CMs | Local ingress through isolation |
| `impulse_events` | Only with a burst source in §8 |

The histogram needs a laser or return amp nonlinearity model, so defer it.

Oracles from source strings with main tap 8 (our arithmetic):

| String | MTC | MTR | NMTER | PreMTTER | PostMTTER | PPESR |
|---|---|---|---|---|---|---|
| CM [PNM-HFC §6.2.6.4.1] | 0.03 | 21.06 | −21.10 | −31.30 | −21.53 | −9.77 |
| CMTS, same | 0.00 | 35.03 | −35.03 | −40.82 | −36.36 | −4.46 |
| Field, 0.5 µs −10 dBc, Table 22 | 0.31 | 11.30 | −11.61 | −24.25 | −11.86 | −12.39 |
| Short plant, Table 22 | 0.03 | 21.27 | −21.31 | −24.71 | −23.96 | −0.75 |

The field string peaks at taps 10 (−17.1 dB) and 11 (−14.4 dB), matching `8 + 0.5/0.1953 = 10.56`, with tap 9 at −25.5 and tap 12 at −24.3 dB. That is the case against single-tap placement.

## Gaps

- Conflicts inside sources: RxMER `0xFF` is no measurement in [CCAP-OSSI §7.3.2.5] but 63.75 dB in [PNM-HFC §6.3.4.3]; we follow the spec. RxMER reference is BPSK in [PHY §9.4.8], QAM in [CCAP-OSSI §7.3.5.5]. PPESR prose says post over pre, while the formula says pre over post and matches the tables. CCAP-OSSI text says eight trigger modes, its Table 434 lists nine. [PNM-HFC §6.2.7] cites [CM-OSSI] D.2.12 for `CmUsPreEq`; I27 has D.2.13.
- Sign and wording: [PNM-HFC §8.11.1] and Figure 276 use MTR as −18 and −25 dB, opposite Tables 26 to 28, apparently meaning NMTER or dBc level. [PNM-HFC §8.11.6.1] pairs low MTR with no serious echo; we read it as high MTR. [PNM-HFC §8.11.3]'s worked trend does not match Table 29, and Tables 29 and 30 hold identical numbers.
- SC-QAM coefficient normalization: only the MTC power statement; the D3.0 PHY was not read, so constant TTE is an assumption.
- No numeric thresholds for OFDMA summary metrics or update magnitude. SC-QAM metrics are not defined for OFDMA, apart from MTR on a match quotient.
- No equation for diplex group delay versus cascade: Figure 139 only, not digitized. Figure 135 was not read; only its text.
- D3.0 power limits come from an article quoted in PNM-HFC, valid for ≥ 32-QAM TDMA only.
- Upstream FEC, impulse noise, histogram and probes are labelled unfielded, so this is spec, not experience. No source maps ingress or impulse level to SNR or codeword loss beyond one field example.
- OIDs other than the one in §10 here were not checked against MIB files.
