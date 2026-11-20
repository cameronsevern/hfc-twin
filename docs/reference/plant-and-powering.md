# HFC plant and powering

How outside plant is built, levelled, measured and powered: cable, passives, actives, alignment, RF metric definitions, demarcation limits and AC power.

Units: MHz, ft, dBmV (75 Ω), °C. Sources quote °F and metric; conversions are stated. Attenuation is dB/100 ft and loop resistance Ω/1000 ft, both at 20 °C (68 °F) unless stated. FCC limits quoted by SCTE 183 apply to NTSC analog channels, and most hfc-twin spectrum is digital, so treat them as background.

## 1. Hardline cable (SCTE 15)

Scope is 75 Ω trunk, feeder and distribution coax with a copper-clad aluminum centre, an aluminum tube shield, and foam or disc-and-air dielectric, specified to 3000 MHz [SCTE 15 §1.1, §1.5, §6.1, §9.1] [normative]. A product type is the shield OD in thousandths of an inch plus F (foam) or D (disc-air), so `.500` foam is 500-F. "P3" in physics.md is a vendor family name that SCTE 15 does not use; mapping it to 500-F is [inferred].

Sizes defined: 412-F, 500-F, 500-D, 540-F, 565-F, 625-F, 650-D, 700-F, 715-F, 750-F, 750-D, 840-F, 860-F, 875-F [SCTE 15 Tables 1-2]. SCTE 32 also lists 440-D, 1000-F, 1000-D, 1125-F and 1160-F [SCTE 32 Table 1].

**Attenuation limits, dB/100 ft at 20 °C.** These are maxima, not typical values [SCTE 15 §15.6 Tables 9-11] [normative]. Checked against the PDF.

| MHz | 500-F | 500-D | 625-F | 650-D | 750-F | 750-D | 875-F |
|---|---|---|---|---|---|---|---|
| 5 | 0.16 | 0.15 | 0.13 | 0.12 | 0.11 | 0.11 | 0.09 |
| 55 | 0.55 | 0.49 | 0.46 | 0.39 | 0.37 | 0.36 | 0.33 |
| 211 | 1.09 | 0.98 | 0.92 | 0.78 | 0.74 | 0.68 | 0.66 |
| 300 | 1.31 | 1.18 | 1.08 | 0.93 | 0.90 | 0.81 | 0.79 |
| 550 | 1.82 | 1.60 | 1.51 | 1.29 | 1.25 | 1.11 | 1.09 |
| 750 | 2.17 | 1.88 | 1.79 | 1.54 | 1.52 | 1.29 | 1.30 |
| 870 | 2.35 | 2.00 | 1.95 | 1.67 | 1.65 | 1.39 | 1.41 |
| 1002 | 2.54 | 2.20 | 2.11 | 1.81 | 1.74 | 1.51 | 1.53 |
| 1218 | 2.83 | 2.45 | 2.32 | 2.02 | 1.95 | 1.68 | 1.70 |
| 1794 | 3.62 | 3.03 | 2.96 | 2.52 | 2.46 | 2.08 | 2.17 |
| 3000 | 4.95 | 4.06 | 4.02 | 3.41 | 3.29 | 2.79 | 3.00 |

**Fits of these maxima to `A(f) = a·√f + b·f`** (physics §1.1) over 5-1218 MHz, weighted by relative error, are ours [inferred]:

| Cable | a | b | Worst error |
|---|---|---|---|
| 500-F | 0.0712 | 0.000283 | 0.01 dB |
| 625-F | 0.0589 | 0.000237 | 0.02 dB |
| 750-F | 0.0483 | 0.000228 | 0.03 dB |
| 875-F | 0.0413 | 0.000226 | 0.02 dB |

These give A(30) = 0.398 and A(750) = 2.16 for 500-F. SCTE 183 uses a simpler √f law for tilt estimation; see §6 below.

**Temperature.** Attenuation rises above 20 °C and falls below it at 0.11 %/°F, which is 0.198 %/°C [SCTE 183 §13.1] [guideline]. This supports k_T = 0.0020/°C. No other coefficient appears in these sources.

**Other electrical limits** [normative]:
- Impedance is 75 ± 2 Ω [SCTE 15 §15.2].
- Velocity of propagation is nominally 87 % for foam and 93 % for disc-air [SCTE 15 §15.4]. Foam takes 1.17 ns/ft one way [SCTE 183 §9.1], matching 1/(0.87 × 983.6). SCTE 183 also treats 87 % as the normal hardline value [SCTE 183 §9.5].
- Structural return loss (SRL) is ≥ 30 dB over 5-1002 MHz, ≥ 24 dB over 1002-1218, ≥ 20 dB over 1218-3000 [SCTE 15 §15.5 Table 8].

**DC resistance at 20 °C, Ω/1000 ft, maximum** [SCTE 15 §15.3 Table 7] [normative]:

| Type | Centre | Shield | Loop | | Type | Centre | Shield | Loop |
|---|---|---|---|---|---|---|---|---|
| 412-F | 2.17 | 0.50 | 2.67 | | 700-F | 0.62 | 0.26 | 0.88 |
| 500-F | 1.42 | 0.37 | 1.79 | | 715-F | 0.61 | 0.45 | 1.06 |
| 500-D | 1.09 | 0.46 | 1.55 | | 750-F | 0.58 | 0.19 | 0.77 |
| 540-F | 1.06 | 0.60 | 1.66 | | 750-D | 0.47 | 0.24 | 0.71 |
| 565-F | 1.01 | 0.36 | 1.37 | | 840-F | 0.45 | 0.18 | 0.63 |
| 625-F | 0.90 | 0.25 | 1.15 | | 860-F | 0.41 | 0.32 | 0.73 |
| 650-D | 0.66 | 0.34 | 1.00 | | 875-F | 0.42 | 0.13 | 0.55 |

Read across Tables 2, 7 and 9, disc-air shows lower loss and centre resistance than foam of the same nominal size, on a slightly larger tube [inferred]. Flooding compound is required below grade and must be non-flowing if used aerially [SCTE 15 §10.1] [normative]. Jacket and armor sizes do not matter to the sim.

## 2. Drop cable (SCTE 74)

Series 59, 6 and 11 braided drop with a copper-clad steel centre and a tape-plus-braid shield in single (tape and braid), tri or quad construction [SCTE 74 §6, §7.2, §13-15; SCTE 32 §1] [normative]. RG6 and RG11 in physics.md correspond to series 6 and 11 [inferred].

**Attenuation, dB/100 ft at 20 °C** [SCTE 74 §11 Table 11] [normative]. The text calls this the maximum attenuation. Checked against the PDF.

| MHz | 5 | 55 | 211 | 300 | 550 | 750 | 870 | 1002 | 1218 | 1794 | 3000 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| Series 59 | 0.86 | 2.05 | 3.80 | 4.45 | 5.95 | 6.97 | 7.57 | 8.12 | 8.99 | 11.06 | 14.30 |
| Series 6 | 0.58 | 1.60 | 3.05 | 3.55 | 4.90 | 5.65 | 6.11 | 6.56 | 7.26 | 8.96 | 11.86 |
| Series 11 | 0.38 | 0.96 | 1.90 | 2.25 | 3.04 | 3.65 | 4.06 | 4.36 | 4.92 | 6.31 | 8.02 |

Drop maxima are almost pure √f, and a free fit yields b < 0 or close to it. With b = 0 over 55-1218 MHz: series 6 a = 0.2085 (worst error 0.06 dB), series 59 a = 0.2589 (0.13 dB), series 11 a = 0.1334 (0.26 dB) [inferred]. For series 11, a free fit (a = 0.1243, b = 0.00040, worst 0.10 dB) is better. All of these under-read at 5 MHz by 0.1-0.3 dB/100 ft (series 6 gives 0.47 vs 0.58), which we attribute to the steel centre conductor [inferred].

**Other electrical limits** [SCTE 74 §11] [normative]:
- Velocity of propagation is 82 % minimum. Impedance is 75 ± 3 Ω.
- SRL is ≥ 20 dB over 5-1218 MHz and ≥ 15 dB over 1218-3000 MHz.
- Maximum DC loop resistance, Ω/1000 ft, in single/tri/quad construction: series 59 is 60.88/58.0/56.1; series 6 is 41.16/38.4/36.9; series 11 is 20.0/17.4/16.7 (Table 10).
- Centre conductor alone: 49.2, 31.1 and 12.3 Ω/1000 ft for series 59, 6 and 11 (§6.4 Table 2).
- The loop value may move by up to 20 % after 10,000 flex cycles [SCTE 74 §13.1].

## 3. Ampacity (SCTE 32)

The tabulated current flows in both conductors. The model is an indoor cable in still air, with conductors reaching 65 °C to limit thermal aging. SCTE 32 warns designers that this temperature raises R, IR drop and I²R, and rules out using the tables to size ground conductors [SCTE 32 §1, §6] [normative].

| Cable | 20 °C ambient | 40 °C ambient | | Cable | 20 °C | 40 °C |
|---|---|---|---|---|---|---|
| 412-F | 33 A | 25 A | | 750-F | 67 A | 50 A |
| 500-F | 43 A | 32 A | | 750-D | 79 A | 59 A |
| 500-D | 49 A | 36 A | | 840-F | 79 A | 59 A |
| 565-F | 50 A | 38 A | | 875-F | 81 A | 60 A |
| 625-F | 54 A | 40 A | | 1000-F | 94 A | 71 A |
| 650-D | 64 A | 48 A | | 1160-F | 117 A | 87 A |

Drops [SCTE 32 Table 2; SCTE 74 Table 12]: series 59 is 6 A at 20 °C and 4 A at 40 °C (SCTE 74 says 4.5 A; quad in SCTE 32 is 5 A). Series 6 is 8 A and 6 A (SCTE 74: 7.8 A and 5.6 A). Series 11 is 13 A and 10 A (SCTE 74: 12.8 A and 9.5 A). Hardline ampacity is well above a typical supply output, so a supply's rating, not hardline ampacity, will usually bind [inferred].

## 4. Taps and passives

These sources describe taps qualitatively and give no datasheet tables. **Insertion loss, tap loss vs frequency, splitter loss and coupler loss are not specified anywhere in this source set** (see Gaps).

- A multitap combines a directional coupler and a splitter. A self-terminating end-of-line tap is a splitter only, usually without a coupler [PNMP §3, Tap] [guideline].
- Return loss: 15-18 dB out of the box and about 7 dB when water-damaged, in a worked example [SCTE 183 §9.1] [field].
- Echo arithmetic: two 7 dB taps 100 ft apart with 1 dB of loss give an echo of 16 dB down, 234 ns late [SCTE 183 §9.1] [field]. This is the same as physics §5.1: R1 + R2 + 2 × span loss, and 2 × 100 × 1.17 ns.
- Tap-port-to-output isolation: a 23 dB tap with 38 dB isolation sends some upstream energy down the line, where it reflects from unterminated low-value tap ports. The echo is more visible on high-value taps [PNMP §5.2.1.2] [field]. Isolation is specified with all ports terminated, and field practice does not always terminate them [PNMP §5.2.1.2] [field].
- Measured isolations: same-tap port-to-port 32 dB (14 dB 2-port) and 29.7 dB (11 dB 8-port end-of-line); tap-to-tap 51 dB (14 dB 2-port to 8 dB 4-port) [TR-MIDSPLIT §6.2.2, §6.2.3] [field].
- FCC minimum terminal isolation is 18 dB, and manufacturer specs may stand in for testing [SCTE 183 §1.9, §12.1] [normative, analog-era rule].
- Corroded seizure screws, tap plates and terminators form diode-like junctions that generate common-path distortion (CPD); see §8 [SCTE 183 §4.3] [guideline].
- Loss of passives usually rises with frequency, and cable loss always does [SCTE 183 §16.4] [guideline]. The PNM guide calls passive insertion loss "flat" as a first approximation [PNM-3.1 §5.2.6.13] [guideline].
- Tap values seen in examples: 23, 20, 14, 11 and 8 dB [SCTE 183 §9.1; PNMP §5.2.1.2; TR-MIDSPLIT §6.2.2] [field].

## 5. Actives: amps and nodes

**Topology.** Levels are nominally flat through the optical link, and the first coaxial amp creates a uniform gain slope [SCTE 183 §6.2] [guideline]. A typical trunk/bridger station runs, from input: diplex filter, input pad, equalizer, preamp, trunk pad, gain and slope controls, trunk output amp. That output feeds the trunk output through a diplex filter, and a bridger pad, bridger amp and bridger splitter. The return runs from a combiner through the upstream amp, equalizer and output pad back to the input diplex filter [SCTE 183 §16.4 Figure 16-13]. Each downstream amp has one input, whereas each return amp may combine several [SCTE 183 §16.4].

**Tilt.** Tilt is added at the node (optical) or headend (coax) to pre-compensate for cable loss, and can read negative at the far end of a run, which is acceptable if within design [PNM-3.1 §5.2.6.13] [guideline].

**Numbers these sources do provide:**
- Test-point loss is typically 20 or 30 dB [SCTE 183 §16.4] [field].
- A 4-output system amp has about 7 dB of extra upstream combining loss. Its noise contribution is about 5× when driven at port level, relative to module-referenced level [SCTE 183 §16.4] [field].
- Return amp noise figure: 10 dB in the worked example [SCTE 183 §16.5] [field]. Device-noise charts use NF 5, 7, 10 and 15 dB [SCTE 183 §3.3 Figure 3-3] [field].
- Peak-to-average ratio matters for loading (§9).
- Do not place digital signals in the roll-off band, where amp group delay and return loss are not guaranteed [SCTE 183 §16.13] [guideline].

**AGC and level control** [SCTE 283] [normative for the reporting model; no ranges given]:
- Downstream AGC type bits: multi-frequency AGC (at least one pilot low and one high in the band), agile (all pilots configurable), and pilot-loss protection, which stops a lost pilot from driving the amp to full gain. Reported by `AgcType` [SCTE 283 §7.2.1.5.7].
- Upstream level control types: quasi-AGC, thermal level control, and a user-selectable thermal level control whose min, max and step are reported in 0.1 dB. Reported by `RfLevelControlType` [SCTE 283 §7.2.1.4.11].
- An AGC can hunt, showing up as a very low-frequency disturbance [SCTE 183 §4.4] [guideline]. High-level sweep can disturb AGC pilots unless notched [SCTE 183 §6.2] [guideline].

**Misalignment signatures.** Boosted bands resembling resonant peaking [PNM-3.1 §5.2.6.9] [guideline]. A full-band capture downstream of a badly misaligned amp shows noise floors that differ between the low unmodulated band and the band above the modulated spectrum [PNM-3.1 §7.6] [guideline]. Too many splits or wrong pads show up as flat loss [PNM-3.1 §5.2.6.14] [guideline]. Unity-gain errors let some spans be thermally limited while others are distortion limited [SCTE 183 §13.5.2] [guideline].

## 6. Forward alignment

Forward amps are aligned by setting output to a predetermined level [SCTE 183 §16.4] [guideline].

- **Unity-gain reference.** Response is measured between two points of nominally 0 dB net gain, excluding intentional tilt (full, half or block) [SCTE 183 §6.2] [guideline]. For all-coax, compare amp output to headend combiner output. For HFC, reference the first coax amp output or normalize out the slope.
- **Response limits.** There is no FCC limit. The guideline for peak-to-valley is N/10 + 2 dB on trunk (N trunk amps), and N/10 + 3 dB including feeder (N all amps). For small-node HFC, N/10 + 3 dB with N = amps after the node [SCTE 183 §6.2] [guideline]. N = 2 gives 3.2 dB, and N = 6 gives 3.6 dB [inferred].
- **Block tilt.** Set at the headend, it should pass through untouched, so align flat to the reference slope regardless [SCTE 183 §6.2] [guideline].

**Sweep methods** [SCTE 183 §6.2] [guideline]:

| Method | Reference level | Character |
|---|---|---|
| High-level simultaneous | 15-20 dB above video carriers | Errors on digital, can saturate amps and read falsely flat; rarely used now |
| Low-level synchronous | About 30 dB below visual carriers, with a pilot | Tracking receiver, shows response and levels together |
| Visual carrier reference | System carriers themselves | Non-interfering; one point per 6 MHz at best; misses fine grain |
| Hybrid | Mixed carriers plus synthesized inserts | Discrete points |
| Vertical interval | About the visual carrier level | Timed into unused video lines |

Coarse sweep resolution can miss the fine-grain ripple from mismatches. High-level sweep can clip a downstream laser [SCTE 183 §6.2] [guideline]. Reflection distance from sweep ripple: **D_ft = 492·Vp / Δf_MHz**, with accuracy improved by averaging across several troughs; multiple faults may instead show the spacing between two faults; ripple depth falls with frequency because of cable loss [SCTE 183 §9.5] [guideline].

**Level at the demarcation and at the home.** Downstream taps are nominally set to the same level, within tap-value increments and spectral variation [SCTE 183 §16.4] [guideline]. See §10 for limits. In the OFDM era:
- The CM input window is −9 to +21 dBmV per 24 MHz, which has the same PSD as −15 to +15 dBmV per 6 MHz. Total input must be under 40 dBmV over 54-1794 MHz [PNM-3.1 §5.1.5, from PHY] [normative via guide].
- OFDM channels usually use the same PSD as SC-QAM [PNM-3.1 §5.1.4] [guideline].
- North America specifies no maximum tilt at the CM input, only the level window. Europe specifies 12 dB, either sign. The reference analyzer's default maximum tilt is 15 dB [PNM-3.1 §5.2.6.13, §5.2.6.13.1] [guideline].

**Carrier-level and tilt math** [SCTE 183 §6.5, §6.6] [guideline]. Add nominal relative offsets to the digital levels (the worked example uses 64-QAM −10 dB, 256-QAM −6 dB). Fit level against √f by least squares. The fit is the ideal level, and residuals are carrier level variations. Cable-only tilt relation: Tilt = A_H·(1 − √(f_L/f_H)), so A_H = Tilt / (1 − √(f_L/f_H)).

## 7. Return alignment and level setting

### 7.1 Principle

Return amps are aligned so all **inputs** at a chosen Reference Point are equal, not outputs [SCTE 183 §16.4] [guideline]. Reference Point options:
- **Module input.** Levels equal at module inputs gives minimum noise addition and distortion build-up, with equal noise from each amp. Different stations need different test insertion levels, and multi-output stations push modems higher.
- **Station port.** Simple and uniform test levels, but module inputs vary. Choose the port with the highest combining loss, and offset others by the loss difference.

### 7.2 Level references

SCTE 183 explicitly suggests a process, not parameters. No level below is normative [SCTE 183 §16.4] [guideline].

| Symbol | Meaning | Relation |
|---|---|---|
| P1 | Total power capability at amp Reference Point | From manufacturer; may depend on cascade depth |
| P2 | Total input capability of optical transmitter | From manufacturer |
| P3 | Alignment test level at amp Reference Point | Common practice **+20 dBmV** |
| P4 | Alignment test level at transmitter input | P4 = P3 − (P1 − P2) |
| P8 | Total output capability of headend optical receiver | From manufacturer |
| P9 | Receiver output test level | P9 = P8 − (P2 − P4) |
| P0 | Level at headend test point during alignment | P9 − A (test coupler loss A) |
| P5 | Service level at amp Reference Point | Chosen, e.g. by power per Hz |
| P6 | Headend test point level for that service | P6 = P0 − (P3 − P5) |
| P7 | Level the CMTS holds at its input | Long-loop AGC target |
| B | Gain from receiver output to data receiver | B = P7 − (P6 + A) |

**Worked anchors, not rules** [SCTE 183 §16.4] [field]: P1 = +26 dBmV and P2 = +16 dBmV, so with P3 = +20 dBmV, P4 = +10 dBmV. A 31 dB line extender test path needs +51 dBmV insertion for P3 = +20 dBmV. P1 = +26 dBmV over 35 MHz gives −49.4 dBmV/Hz, so 2 MHz is +13.6 dBmV. P0 = +12 dBmV with P5 = +5 dBmV gives P6 = −3 dBmV.

**Procedure.** Characterize test-point-to-reference loss per station type, choose totals, align the optical link with a test signal, then work outward from the node. At each station, insert to reach P3 at its reference and adjust the upstream output pad and equalizer until the headend reads P0. Do amps nearer the node first [SCTE 183 §16.4] [guideline].

**Headend gain and long-loop AGC.** The CMTS commands modems so they all arrive at P7. Too much gain between optical receiver and CMTS leaves plant levels low and CNR degraded. Too little makes modems transmit too high and may overload plant [SCTE 183 §16.4] [guideline]. The CMTS port is typically set for 0 dBmV per 6.4 MHz [PNM-3.1 §6.1.2.4] [guideline].

**Constant power per Hz.** A popular allocation that gives all services the same CNR if noise is flat and susceptibility bandwidth equals occupied bandwidth. Peak voltage, not average power, ultimately limits signals, so include headroom [SCTE 183 §16.4] [guideline].

### 7.3 Translating to 6.4 MHz

The constant-power rule implies P5 = P1 − 10·log10(BW_total / 6.4 MHz) per 6.4 MHz [inferred from SCTE 183 §16.4]. Using the +26 dBmV amp and +16 dBmV laser example totals [inferred]:

| Return band | BW used | P5 at amp reference, per 6.4 MHz | At laser input, per 6.4 MHz |
|---|---|---|---|
| 5-42 MHz | 37 MHz | 18.4 dBmV | 8.4 dBmV |
| 5-85 MHz | 80 MHz | **15.0 dBmV** | 5.0 dBmV |
| 5-204 MHz | 199 MHz | 11.1 dBmV | 1.1 dBmV |

### 7.4 Laser limits and midsplit data

From field testing [TR-MIDSPLIT] [field]:
- Analog DFB laser NPR curves show a 7 to 27 dBmV usable total input range for 1024-QAM, with MER 35.5 dB at 7 dBmV and 52.5 dB at 26 dBmV (§6.1.1.1 Table 2). Digital lasers: about 11 to 25 dBmV (§6.1.1.2).
- With clipping margin for 1024-QAM and 38 dB node SNR, target total input is 10 to 22.6 dBmV on analog DFB and 13 to 20.6 dBmV on digital (§6.1.2).
- Moving 5-42 to 5-85 with constant total power: four 6.4 MHz channels at 12 dBmV (18.02 dBmV total) becomes ten channels, a PSD drop of 3.98 dB (§5.1), which is 8.02 dBmV per channel [inferred arithmetic].
- CM transmit into in-home legacy tuners (54-85 MHz): in-home isolation is 9-25 dB. At best-case isolation, up to 46 dBmV/6.4 MHz is safe, but more likely not much over 30 dBmV/6.4 MHz (§6.2.1).

### 7.5 Modem transmit range

Upstream loss from far subscribers can be as much as 20 dB less than from those on the first tap. To reduce required range: lower high-value taps, add in-line equalizers, avoid low-value taps on new builds (also reduces ingress sensitivity), use unequal in-home splits, or add two-way drop amps [SCTE 183 §16.4] [guideline].

### 7.6 Upstream noise prediction

[SCTE 183 §16.5, §3.1] [guideline]:
- Thermal: −125 dBmV/Hz; −59 dBmV in 4 MHz at 20 °C. Section 3.3 uses −59.2 dBmV at 17.5 °C.
- All amps funnel: `N_coax = −125 + 10·log10(BW_Hz) + NF + 10·log10(M)`, with M the total amps in the node, not cascade depth. Example: 35 MHz, NF 10 dB, M = 32 gives −24.6 dBmV at the node input.
- Mixed stations: sum `M_x · 10^(N_x/10)`, with NF relative to the chosen reference.
- Optical link, normalized to node input: `N_opt = C2 − (C/N)2 + 10·log10(BW_T/BW_2) + (P1 − P2)`, independent of amp count. Example equivalent: −29 dBmV, about a 10-amp cascade.
- Total: linear sum of `N_coax` and `N_opt`.
- With power per Hz, a service's CNR ≈ total power / total noise, as long as noise is ≥ 10 dB below power.
- Low operating limit comes from funneling; the high limit from laser clipping or amp compression.

## 8. Noise and distortion definitions

- **Noise level.** True average power in the stated bandwidth, in dBmV [SCTE 183 §3.1] [guideline].
- **Thermal noise.** The noise floor excluding digital intermodulation. For optics, it may include shot and RIN [SCTE 183 §3.1].
- **Intermodulation noise (IMN).** Noise-like products from a nonlinear system carrying digital signals. They depend on signal level [SCTE 183 §3.1].
- **Composite noise.** Thermal plus intermodulation noise [SCTE 183 §3.1].
- **CNR, digital.** Channel power over average noise in occupied bandwidth. Es/N0 × 1/(1+α) = CNR, with α the filter excess bandwidth [SCTE 183 §14.1, §14.2].
- **CCN.** Average digital signal power over composite noise, in the same bandwidth. No bandwidth correction is needed because the signal is noise-like [SCTE 183 §3.5].
- **Noise-near-noise correction.** `10·log10(1 − 10^(−drop/10))` dB. Drop under 2 dB is unreliable; add 4.3 dB and report ">" [SCTE 183 §3.1]. 10 dB drop gives 0.46 dB.
- **Device CCN.** `CCN_out = C_in − 10·log10(10^((C_in − CCN_in)/10) − 10^(−59.2/10) + 10^((−59.2 + NF)/10))` for 4 MHz noise bandwidth [SCTE 183 §3.3] [guideline]; checked against the PDF, where the first exponent's subscript prints as CCN_VI and the figure axis shows C_VI, so we read it as C_in.
- **CSO.** Second-order composite beat cluster to carrier. Falls at 0.75 or 1.25 MHz offsets, worst at band edges. Push-pull and feed-forward coax amps minimize it, and single-ended fiber or microwave gear raises it. Modulated carriers measure about 6 dB better than CW [SCTE 183 §4.1.1].
- **CTB.** Third-order composite beat cluster to carrier. Modulated measures about 12 dB better than CW, and designs are specified at CW because it repeats (modulated varies 2-4 dB). Largest counts just above mid-band: beats ≈ 3N²/8 mid-band, N²/4 at edges, for N channels [SCTE 183 §4.1.1, App. A].
- **Combining distortions.** Worst case is voltage addition, `20·log10(10^(D1/20) + 10^(D2/20))`; actual lies between 10·log and 20·log. Example: −75 and −56 dBc gives −55.1 dBc [SCTE 183 §4.1.1]. Uncorrelated products add in power [SCTE 183 §4.1 App. B].
- **Analyzer bias.** Up to 2.5 dB low for noise-like products under the standard log-detect method [SCTE 183 §4.1].
- **CIN.** Carrier to intermodulation noise, resembling thermal noise statistically. Depends on amp level, linearity, and count and placement of digital carriers [SCTE 183 §13.5.2, §16.10].
- **IIN.** Interferometric intensity noise; optical path effect [SCTE 183 §16.10].
- **NPR.** A band of Gaussian noise spanning the return, with a notch; NPR is notch depth. Includes thermal and intermod, can't separate them. Curve rises on the noise side, then falls on the clipping side; dynamic range is the input power span that holds a target NPR. Out-of-service only [SCTE 183 §13.5.2].
- **SSO, STO.** Upstream two-carrier tests. Products A+B, B−A, 2A−B, 2B−A [SCTE 183 §4.2].
- **CPD.** Second-order products at multiples of 6 MHz; third-order at ±1.25 MHz around those (non-HRC) in 6 MHz plans. Elusive and intermittent, generated at corroded contacts and most visible in the return [SCTE 183 §4.3].
- **MER.** `10·log10(avg symbol power / avg error power)` [SCTE 183 §11.3]. Also called SNR on many modems; cluster variance is equivalent [SCTE 183 §11.3, §14.3]. `EVM% = 100 · 10^(−(MER + MTA)/20)`, MTA 0, 2.55, 3.68, 4.23, 4.50 dB for QPSK, 16, 64, 256, 1024-QAM; limit 4.77 dB [SCTE 183 §11.3 Table 11-1].
- **MER vs CNR.** Equalized MER equals CNR only under pure AWGN. Otherwise MER is at least a few dB lower, and good CNR with poor MER points at something other than noise [SCTE 183 §11.3]. MER needs recovery of data and is limited by the test receiver [SCTE 183 §16.9].

## 9. Loading and peak

Total power: `P_total = P_single + 10·log10(N)`, or the linear sum if unequal [SCTE 183 §2.3] [guideline]. Peaks for N randomly-phased CW carriers: peak power is 10·log10(2N²) above a single carrier's average. A flat composite of digital channels has a Gaussian-like distribution: 13 dB over average at probability 1e−5. Ten CW carriers exhibit 13 dB and come within 2 dB of real operation. 4 CW carriers understate peaks by about 6 dB [SCTE 183 §16.2] [guideline]. Set return power from NPR or a peak analysis, not from power sums of few-carrier specs [SCTE 183 §16.4] [guideline].

## 10. Demarcation limits (SCTE 40)

Applies at the premises at the end of a terminated drop, 54-1002 MHz with 6 MHz channels. Upstream may be any of 5-30, 5-42 or 5-85 MHz. DOCSIS appears only as an undesired signal [SCTE 40 §1, §6.2, §6.3.5, §6.4] [normative]. Table 4 was checked against the PDF.

| Item | Limit |
|---|---|
| Transit delay | ≤ 0.800 ms |
| C/(N+I) in 6 MHz, all impairments included | ≥ 27 dB for 64-QAM, ≥ 33 dB for 256-QAM |
| C/N analog | 43 dB |
| CTB, CSO on analog | ≤ −53 dBc |
| Other discrete interference | ≤ −53 dBc |
| AM hum | ≤ 3 % p-p |
| Group delay variation | ≤ 0.37 µs/MHz across 6 MHz |
| Phase noise | ≤ −86 dBc/Hz at 10 kHz |
| Amplitude variation in 6 MHz | ≤ 6 dB p-p digital, ≤ 4 dB p-p analog |
| Single dominant echo | −10 dB ≤ 0.5 µs, −15 dB ≤ 1.0 µs, −20 dB ≤ 1.5 µs, −30 dB ≤ 4.5 µs |
| Level at terminal: 64-QAM | −15 to +15 dBmV |
| Level at terminal: 256-QAM | −12 to +15 dBmV |
| Level at terminal: analog visual | 0 to +15 dBmV |
| Level at terminal: QPSK FDC | −15 to +15 dBmV; C/(N+I) ≥ 20 dB in Nyquist BW |
| Any single signal > 54 MHz | < +20 dBmV |
| Any single signal 0.5-30 MHz | ≤ +42 dBmV |

Echoes over 4.5 µs count as interference in C/(N+I). Between 30 and 54 MHz no limits are defined [SCTE 40 §6.1.1, Table 2, Table 4]. Relative carrier power to analog: 256-QAM −5 ± 2 dB, 64-QAM −10 ± 2 dB, QPSK FDC −8 ± 5 dB, each carrier ± 3 dB of nominal [SCTE 40 §6.4.1.1 Table 5].

**FCC analog proof-of-performance, as reproduced in SCTE 183** [normative for NTSC]. Visual level ≥ 0 dBmV at terminal and ≥ +3 dBmV at the end of a 100 ft drop. Variation ≤ 8 dB over 6 months, tested each 6 hours across a day in Jan/Feb and in Jul/Aug. ± 3 dB to carriers within 6 MHz; 10 dB across the plant up to 300 MHz, plus 1 dB per 100 MHz. C/N ≥ 43 dB. Coherent disturbance ≥ 51 dB (47 dB for frequency-coincident on coherent systems). Hum ≤ 3 % p-p. In-channel ± 2 dB. Leakage: 15 µV/m at 30 m for ≤ 54 and > 216 MHz (13.1 µV/m digital); 20 µV/m at 3 m for 54-216 MHz (17.4 µV/m digital) [SCTE 183 §1.1-§1.12].

## 11. Hum and other RF measures

**Hum** is unwanted amplitude modulation under 1 kHz from the power system, stated as peak-to-peak percent of carrier peak [SCTE 183 §4.4] [guideline]. Main sources of 60 and 120 Hz: amps with defective module supplies and overloaded system supplies. Passives can also contribute. Switching regulation produces the same effect at higher rates [SCTE 183 §4.4] [guideline]. Limit is 3 % p-p for both analog (FCC) and digital [SCTE 183 §1.10, §4.5; SCTE 40 Table 4].

On QAM [SCTE 183 §4.5] [guideline]: the receiver's AGC (tens of ms) and equalizer main tap track moderate hum at no visible cost, then errors arrive abruptly at the FEC cliff. Measurement is zero-span, 1 MHz RBW, 3 kHz VBW, 20 ms sweep, line trigger, 100 averages, then a 0.5 kHz low-pass, with p-p taken at the 0.5 and 99.5 percentiles. Example result: 0.33 dB p-p. Formula: `Hum% = 100·|1 − 10^(Hum_dB/20)|`, where the source's printed form yields a negative value and we keep the magnitude. 0.5 dB gives 5.9 %; 3 % ≈ 0.26 dB [inferred].

**Other measures:**
- Group delay and in-channel flatness: limits in §10; digital in-channel tilt scales by dB/MHz × 6 [SCTE 183 §6.4].
- Propagation delay: DOCSIS allows 0.8 ms one way, about 168 km of fiber [SCTE 183 §8.1].
- Coax TDR: ± 2 % distance error normal; 10 ns pulse; qualitative only, not an SRL measurement [SCTE 183 §13.2].
- Longitudinal sheath current: neutral current shared through bonded cable sheath, can be comparable to powering current, and develops voltage that shifts supply excitation. A recorded clamp-on measurement over a long interval identifies it [SCTE 183 §12.4] [guideline].

## 12. AC powering

### 12.1 Supplies and waveform

Power is line-frequency AC superimposed on the coax: 30 VAC historically, then 60 VAC, now 90 VAC [SCTE 183 §16.12] [guideline]. 60 and 90 VAC sources, with or without battery standby, power nodes and RPDs alike [TR-DCA §11.8] [guideline]. Amps report line voltage in 1 VAC units through the optional objects `rfAmpLinePowerVoltage1` and `rfAmpLinePowerVoltage2` [SCTE 38-10, rfAmplifierIdent] [normative]. Amps are constant power [SCTE 183 §16.12] [guideline].

| Supply type | Construction | Behaviour |
|---|---|---|
| Non-standby ferroresonant | Input winding, resonant capacitor tap, output tap, saturable core | Any input loss interrupts output |
| Ferroresonant standby | Adds inverter winding, battery string, control | Battery carries utility outage |
| Switch-mode standby | Charger, batteries, inverter, high-frequency switching | Same quasi-square wave without ferroresonant transformer |

Source: [SCTE 183 §16.12] [guideline]. Ferroresonant facts from the same section:
- Input 117 VAC sine; output is **quasi-square wave**, approaching sine as load rises to 110 % of rating.
- Regulation is 1-2 % output change for 15 % input change; better at light load.
- Efficiency approaches 90 % at full load.
- **Design load is 80 % of rating**, which improves regulation and leaves headroom for startup, when constant-power amps pull extra current at low voltage.
- Small unregulated generators (< 10 kW) don't drive them reliably.
- Non-sinusoidal output requires a true RMS meter for all voltage, current and power readings; a non-RMS meter is the most common cause of false alarms.

### 12.2 Batteries

Sealed valve-regulated gel lead-acid is usual, as three 12 V batteries (36 VDC) or four (48 VDC) [SCTE 183 §16.12] [guideline]. A charged three-battery string gives about 90 minutes standby and about 24 hours to recharge from empty. Charge ceiling: 13.8-14.1 V per battery at 20 °C. Discharge floor: 10.2 V per battery, below which damage is irreversible. Temperature is often the top life factor, and temperature compensation prevents thermal runaway. Over 90 % of life is on float [SCTE 183 §16.12] [guideline].

Charger example: 36 V string, 40.5 VDC nominal at 25 °C, 18 cells, 0.005 V/°C/cell, so 41.4 VDC at 15 °C [SCTE 205 §10.2] [guideline].

### 12.3 Preventive maintenance

Visit each supply at least annually, with battery work at 0-38 °C [SCTE 205 §8, §10] [guideline].

| Test | Criterion | Citation |
|---|---|---|
| Float at 0 / 12.5 / 25 / 31.5 / 38 °C: OK if above | 13.6 / 13.4 / 13.1 / 13.0 / 12.8 VDC | SCTE 205 §10.3 Table 1 |
| Float: replacement candidate if below | 13.0 / 12.8 / 12.5 / 12.4 / 12.2 VDC | SCTE 205 §10.3 Table 1 |
| Conductance, temperature-adjusted +0.7 % per 1.11 °C below 25 °C | Replace under 40 % of new; 40-80 % marginal, needs self-test | SCTE 205 §10.4 |
| 10-minute self-test, read at minute 9 | Under 10.8 VDC: replace battery if < 2 years, else string; > 0.3 V between batteries needs action | SCTE 205 §10.5 |
| Warehouse redeploy, 14.40 VDC for 12 h, 24 h rest | Below 12.60 VDC recycle; redeploy within 60 days | SCTE 205 §10.6 |
| Bonding | #6 AWG minimum ground wire | SCTE 205 §9.4 |
| Air gap between batteries | 13 mm recommended, 7 mm minimum | SCTE 205 §10.1 |

### 12.4 Failures and alarms

Example alarms [SCTE 205 §13 Table 2] [guideline]:

| Alarm | Causes |
|---|---|
| Self-Test Fail | 36 V string < 33.0 VDC, 48 V string < 44.0 VDC, inverter failure, line isolation |
| Low Batt Volts | Bad batteries or same thresholds, unseated or failed inverter module, AC input fail |
| High Batt Volts | 36 V string > 45.0 VDC, 48 V string > 60.0 VDC, charger settings |
| Output Failure | Battery end of discharge during line failure, output > 100 %, short, config |
| Output Overload | Overloaded or shorted output |
| Line Isolation | Input relay failure |
| Tap Fuse Fail | Open automatic transfer switch fuse, relay damage |
| Input Failure | Utility AC failed, input breaker, connections |
| N+1 In Use / Fault | Primary output failed and load moved / standby unit not producing |
| Output Overvoltage Reading | Config error, metering, transformer |
| Battery Temp Probe | Sensor or inverter board failure |

Other failure and inspection points [SCTE 205 §9.1-§9.6, §10.1, §11.1, §12] [guideline]: leaking, cracked, swollen or corroded batteries; mixed strings; dried inverter capacitor (display absent); burnt or discoloured connectors, which indicate excess current; dead surge protector; undermined pads; flood plain; corrosion; transponder RF level. The status-monitoring transponder is checked in the same visit [SCTE 205 §12]; its fields belong in collection-management.md. DAA makes even short interruptions into extended outages [SCTE 205 §1.1] [guideline].

## Implications for hfc-twin

All [inferred].

**physics §1, component `Cable` fields `atten_a`, `atten_b`, `k_temp`, `v_f`, `r_loop_ohm_per_kft`.**
- SCTE 15 and SCTE 74 give ceilings. physics §1.1's hardline rows run 0.90-0.95 of ceiling, which is plausible as typical. RG11 is above its ceiling at all four frequencies (1.02-1.09×), and RG6 slightly at 1000 and 1218 MHz (1.01-1.02×). Correct those, or tag them as not spec-compliant.
- The physics .500 coefficients (a = 0.0598, b = 0.000460) are more dielectric-weighted than the ceilings imply (a = 0.0712, b = 0.000283). At 30 MHz they give 0.341 against a 0.398 ceiling fit, so about 14 % lower, against 6-8 % lower from 750 to 1218 MHz. This matters to §7.2 upstream and to the oracles that use 0.341.
- Generic library entries can cite SCTE 15 and 74 ceilings directly as `spec_confidence: Generic`. If the fixture moved to ceilings, the §3.2 oracle modem at 750 MHz goes from 1.21 to about −0.5 dBmV, still legal, but the −3.41 dBmV low end would need rerunning.
- `k_temp` = 0.0020 and foam `v_f` = 0.87 are sourced. For drops, `v_f` = 0.85 is permitted but only ≥ 0.82 is specified; v_f error of 0.03 gives 3.6 % in fault distance.
- Add disc-air (`v_f` 0.93) if the library ever includes -D types. Only they break 0.87.
- SCTE 32's warning implies that `r_loop` rises with temperature. The sim uses 20 °C values and physics §6.2 ignores temperature. Resistivity about 0.4 %/°C (standard metal value, not in these sources) would add about 10 % at 45 °C. That equals the ±10 % §8 jitter. Consider adding it in step 3.

**physics §2, `Tap` and passive fields.** All tap and splitter losses stay `Generic` until datasheets arrive; nothing here verifies them. Tap RL defaults of 15-18 dB fit physics §5.1's 16-20 dB within reason. For a water-damaged fault, use 7 dB as a seed value in fault sampling. The 20 dB `Port isolation` remark is consistent with FCC 18 dB and measured 29.7-32 dB (same tap). 51 dB tap-to-tap is a test oracle for second-order effects if ever modeled.

**physics §3, `design_levels`.**
- `modem_in_min_dbmv`/`modem_in_max_dbmv` stay −15/+15 for CMs (PNM-3.1). SCTE 40's −12 dBmV floor is for 256-QAM video terminals, so it matters only if a profile models set-tops.
- `tilt_tolerance_db` 15 is right as a software default, but its section is §5.2.6.13.1 in PNM-3.1 rev 05.
- N/10 + 3 gives a defensible default ripple tolerance for jitter and a design check on response.
- FCC's 8 dB seasonal rule is a useful test that the fixture's temperature swing stays reasonable, but it only strictly applies to analog.

**physics §3.4, `agc_range_db`.** No numeric range exists here. Worth modeling later as two pilots (level and slope), since multi-frequency AGC is the modern type, and pilot-loss to full gain as a fault.

**physics §4.** Cascade rules are not stated by source, and §4.4's CIN slope stays approximate. Since CIN is third-order dominated in push-pull amps, IM3 theory would give 2 dB per dB rather than the 1 dB per dB that physics uses, and that should be re-examined with datasheet data. Noise-like additivity supports 10·log N. CTB's 20·log N matches the worst-case combination; CSO's 15·log N sits inside the stated 10-20 range. SCTE 183's device formula adds (F − 1)·kT to an input that already carries thermal noise, whereas physics §4.2 uses F·kT per stage. Across a high-loss span between unity-gain amps the two nearly agree, since the stage term is F − 1/L. §4.5's MER structure agrees with §8 here. SCTE 40's C/(N+I) of 27 and 33 dB is a video-QAM floor, useful as a sanity check only.

**physics §5.1.** Round-trip echo math and 492·v_f formula are confirmed. SCTE 40's echo bounds are a usable severity check on downstream echoes.

**physics §6, `power_w`, `v_min`, `powering`.**
- Constant-power load is confirmed, and that includes the startup current surge. `v_min` is not in any source here. Tag as `Generic`.
- `I_rated` 15 A is also unsourced. Add a design rule: Σ I ≤ 0.8 × rating. Add hardline ampacity (43 A/32 A for 500-F) as a check that should rarely bind, and an RG6 limit of 5.6-8 A if drops ever pass current.
- Battery standby can be derived: three-battery string, 90 minutes, floor 10.2 V per battery, charge 24 h. These suit a `PowerSupply` fault scenario. Hum at the modem from an overloaded supply is real, but no term exists in physics §8. Treat as a v2 addition.

**physics §7, `us_node_in_target_dbmv`, `return_gain_db`.**
- `P_in,node` corresponds to SCTE 183's P5, the service level at an amp reference, because amps restore unity back to it. **15 dBmV per 6.4 MHz is plausible** on that reading: it is exactly the constant-power level for the +26 dBmV example on a 5-85 MHz return.
- Literally at a laser input, it would be high: SCTE 183's offset makes that about 5 dBmV, TR-MIDSPLIT's example 8 dBmV, and its 22.6 dBmV total limit about 11.6 dBmV. Record the Reference Point choice in the profile text.
- The 5-204 equivalent is about 11 dBmV, so the constant should vary by split. CMTS 0 dBmV is P7, and physics' "return path sets how much higher" is B.
- physics §7.3 lacks thermal funneling. Add `N_coax` from §7.6 here, with a return noise figure (10 dB example). The component spec has only `nf_db`, so consider `return_nf_db`. Fixture example: M = 2, NF 10 dB, 6.4 MHz gives −44.1 dBmV, so about 59 dB SNR at 15 dBmV. M counts all amps in the node, not cascade depth. Add optical `N_opt` if the node is analog.
- The §7.2 oracle of 37.54 dBmV/6.4 MHz at 30 MHz is fine, but TR-MIDSPLIT's roughly 30 dBmV soft concern for in-home legacy tuners, applicable to 54-85 MHz, may matter for the profile if it models legacy devices.

## Gaps

- **No source here gives amp specs:** gain, tilt range, noise figure (forward), output capability, AGC range, min input level, node RF output, power draw. physics §3.1, §3.4, §4.2 and §6.1 values stay unverified. SCTE 279 (AGC types) and SP 920 (upstream level control) are referenced by SCTE 283 and not in this set.
- **No source gives** tap insertion or tap-loss tables, splitter or coupler loss. physics §2 stays unverified.
- **Voltage minimums** (physics §6.3 40-45 V and 60-65 V) and **15 A supply rating** are in none of these documents. Neither are typical supply VA ratings or power-inserter behaviour.
- **CTB, CSO and CIN level slopes (dB per dB) and cascade factors** are not stated in SCTE 183. It gives definitions, CW-vs-modulated offsets and a 10·log to 20·log combining range. SCTE 17 (the CIN procedure) is referenced, not read.
- **k_T is the one temperature number.** No source splits skin vs dielectric temperature dependence, nor gives per-span aerial vs underground temperature.
- **Downstream design targets** (node/amp output level, standard tilt for 1218 MHz plant) are absent. SCTE 40 stops at 1002 MHz and pre-dates OFDM; extending its limits to 1218 MHz would be our call.
- **Source oddities:**
  - SCTE 205 §10.2 states float tolerance as ± 0.3 mV, likely meaning 0.3 V.
  - SCTE 205's 40.5 V string (13.5 V per battery at 25 °C; 13.65 V at 20 °C with its compensation) sits below SCTE 183's 13.8-14.1 V at 20 °C.
  - SCTE 183 §4.5's hum formula has a sign slip. Its 5.9 % example is the magnitude.
  - SCTE 74 Table 11 series 6 at 1800 MHz reads 9.5 between 8.96 and 9.40, seemingly a typo, and §13 cites Table 11 for loop resistance when it is Table 10.
  - SCTE 15 Table 2 650-D shield thickness of 0.195 in reads as a typo for 0.0195 in.
  - SCTE 32 and SCTE 74 ampacities differ by up to 0.5 A; SCTE 74 is the newer.
