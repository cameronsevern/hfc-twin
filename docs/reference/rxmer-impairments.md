# RxMER per subcarrier: what impairments look like in the field

The curve shapes and sizes operators publish, set against what our sim draws. Load this for anything that simulates, labels or
classifies RxMER spectra. `impairments.md` covers causes, look-alikes and repair; this file covers the curve.

Units: MHz, dB. **Figure readings are by eye from small plots, about ±2 dB, and tagged [figure].** No source below publishes these
numbers as a table. They are examples, not distributions.

## 1. Who has done this

| Source | Data | Classes | Model | Result |
|---|---|---|---|---|
| [ZHU2020] CableLabs | 45,000 train, 26,000 test OFDM captures, human-labelled plus synthetic, 1,800–2,000 values each, 0–63.75 dB | LTE ingress, wave, roll-off, suck-out, spike | 1-D fully convolutional detector modelled on YOLOv3: class, centre, width, several per capture | 97.82 % mAP. Weak on spikes, dense small anomalies, a very subtle wave, one large roll-off |
| [FERREIRA2020] Comcast | About 4,000 labelled OFDM captures, 48–96 MHz wide, about 1,400 normal | Normal, ingress, sweep, roll-off, suck out, amplitude ripple, low MER; analog TV distortion and adjacent channel too rare to score | 1-D CNN, multi-label | Subset accuracy 0.852, against 0.645 for 3 dense layers. Recall: ripple 0.715, suck out 0.911, low MER 0.924, ingress 0.935 |
| [FERREIRA2023] Comcast | About 19,500 labelled OFDMA captures, 7,733 normal | VHF channels 2–6, analog modulator channels 3 and 4, RFoG, Other | 1-D CNN, multi-label | ROC near 1 on holdout. "Other" holds impairments nobody has identified |

Both operators' models are neural and work on the raw curve, so a neural-only rule matches the published work [inferred].

- **Several impairments per capture are normal.** About 60 % of Comcast's labelled samples hold a single impairment, and most errors came from multiple
  impairments [FERREIRA2020 §5, §8].
- **Sampling.** Comcast sampled for high variance over subcarriers, because most captures have no impairment [FERREIRA2020 §3; FERREIRA2023]. So these counts are not
  prevalence. **Nobody publishes prevalence.**
- **Severity is not solved.** Comcast's labelling tool records low, medium, high, not used in the models, and severity is listed as future work [FERREIRA2020 §3, §10].
- **Many patterns are transient.** Polled about every 5 minutes for 6 days, one device showed its pattern a few times a day, and another on 2 days of 6 [FERREIRA2023 §2.2].
- **Unknowns are real.** Comcast shows a set of signatures still unidentified, grouped by node segment [FERREIRA2023 Fig 5].
- **CableLabs' synthetic rows are scripted** (sine waves for waves, non-linear functions for suck-outs) [ZHU2020 §IV.B]. Invariant 5 forbids that here: our shapes must come out
  of the physics.

## 2. Field shapes and sizes

| Family | Source | Baseline | Floor | Drop | Width | Notes |
|---|---|---|---|---|---|---|
| Healthy | [HRANAC2019 Figs 6, 10] [figure] | 40–45 | | | | Visibly noisy, about ±1–2 dB, with occasional narrow dips |
| Healthy, OFDMA | [FERREIRA2023 §2] | Over 40 | | | | Essentially flat, stated in the text |
| Wireless ingress | [FERREIRA2020 Fig 1] [figure] | 42 | 33 | 9 | About 10 | |
| Ingress | [HRANAC2019 Fig 9] [figure] | 40 | 34 | 6 | About 8 | Near 890 MHz |
| Ingress, simulated in lab | [HRANAC2019 §2.1.1] | | | About 10 | 10 | Stated in the text |
| LTE, several bands | [ZHU2020 Fig 3] [figure] | 30 | 10 | About 20 | 5–15 each | The deepest published |
| Suckout | [FERREIRA2020 Fig 1] [figure] | 38 | 20 | 18 | About 15 | Recovers to 32, so the sides differ |
| Suck-outs | [ZHU2020 Fig 4] [figure] | 40 | 27, 28 and about 8 | 13 to 30+ | 5–20 | Three in one capture, described as rare |
| Wide dip, unnamed | [FERREIRA2020 Fig 1, "Other"] [figure] | 38 | 20 | 18 | 60+ | |
| Roll-off | [FERREIRA2020 Fig 1] [figure] | 42 | 22 | 20 | Last 20 | |
| Roll-off | [ZHU2020 Fig 6] [figure] | 38 | 15 | 23 | Last 30 | |
| Ripple | [FERREIRA2020 Fig 1] [figure] | 32 → 25 | | 5 p-p | Period about 7 | On a falling baseline |
| Ripple | [HRANAC2019 Fig 12] [figure] | 36–42 | | About 5 p-p | Period about 7.5 | |
| Wave, non-periodic | [ZHU2020 Fig 5] [figure] | 38 → 25 | | About 5 p-p | Whole channel | |
| Low MER | [FERREIRA2020 Fig 1] [figure] | 30 → 25 | | 10–15 below healthy | Whole channel | |
| Adjacent channel, edge | [FERREIRA2020 Fig 1] [figure] | 40 | 28 | 12 | First 3 | |
| Edge roll-off | [HRANAC2019 Fig 10] [figure] | 44 | 38 | 6 | First 2–3 | Not plant damage, see §3 |
| Spike | [ZHU2020 §IV.A, Fig 6] | 38 | About 28 [figure] | About 10 | Under 3 subcarriers | CableLabs calls them minor, not worth a technician |
| Sweep generator | [FERREIRA2020 Fig 1] [figure] | 37 | About 20 | About 17 | Narrow comb, about every 6 MHz | |
| VHF ingress, OFDMA | [FERREIRA2023 §2.1.1] | | | | TV channels 2–6 | One or several per device |
| Analog modulator, OFDMA | [FERREIRA2023 §2.1.2] | | | | 61.1 or 66.1 MHz | Old VCR, console or set-top in the home |
| RFoG, OFDMA | [FERREIRA2023 §2.1.3] | | | | Much wider than VHF | A departed customer's fiber equipment left connected |
| Multiple | [HRANAC2019 Fig 11] [figure] | 42 → 28 | About 26 | | | Edge roll-off, several ingress bands, a falling baseline |

So field drops of 15–30 dB are in the published record, on roll-offs, LTE and suckouts, and the channel mean hides many of them. A 20 dB drop is squarely inside this.

Related, from `impairments.md`: water-soaked tap about 30 dB level loss by 750 MHz; water-damaged drop about 20 dB down by 700–850 MHz, with repair restoring RxMER to 40+;
laser test RxMER 37 to 23; suckouts of about 20 dB in level at 715 MHz; severe wide case 210 MHz at 560 MHz. A modem below about 24 dB likely cannot use the OFDM channel at all
[broadbandlibrary.com/rxmer]. That fits a 256-QAM boot profile, and a lower one should hold further down [inferred; `docs/physics.md` §7.4].

## 3. Why ripple shows or doesn't: the lab result

[HRANAC2019 §3.1, Appendix I] tested a micro-reflection with about 8 MHz ripple spacing, at −10, 0 and +10 dBmV.

- **Noise before the reflection** passes through the echo with the signal, and the equalizer flattens both, so RxMER stays flat (Test Case 3).
- **Noise after the reflection** isn't shaped by the echo. The equalizer inverts it, RxMER shows the original ripple (Test Case 4).
- **The modem's own noise counts as after.** The paper's list of what makes it up: amplifier noise, A-D converter precision, phase noise, quantizing error [HRANAC2019 §3.1.1].
- **Echo alone, no added noise** (Test Cases 2B and 2C) [figure, values in plot headers]:

| Input | Channel estimate ripple | Mean RxMER | RxMER std dev | RxMER ripple |
|---|---|---|---|---|
| 0 dBmV | 6.97 dB p-p | 46.65 dB | 1.57 dB | About 4–5 dB p-p |
| +10 dBmV | | 46.66 dB | 1.62 dB | About 4–5 dB p-p |

Raising the level 10 dB changed nothing, so this isn't thermal noise. It's a ceiling, near 47 dB on this modem, **and that ceiling ripples** [inferred]. A sine of 7 dB p-p has a standard deviation of about 2.5 dB, so 1.6 dB
says most of the ceiling is shaped, not all of it [inferred].

**Edge roll-off** comes from the OFDM taper region overlapping an adjacent SC-QAM, set by Ncp and Nrp. Moving from Ncp 192, Nrp 128 to Ncp 1024, Nrp 256 reduced it [HRANAC2019 §3.2]. Some modems
show it and others on the same signal don't. A classifier must not call it plant damage.

## 4. Our sim against the field (2026-11-04, after task 16)

Before task 16 the sim drew no ripple at all, even with two 3 dB reflectors, kept water flat, and cut roll-offs off at a cliff. `crates/sim/tests/field_shapes.rs` now holds every family at its target, and [the gallery](../rxmer-gallery.md) shows the curves.

**Still different from the field:**

- Ingress bands have square edges and a flat floor. Real ones are rougher.
- Healthy curves are flat apart from scatter. The public capture slopes −1.18 dB per 100 MHz.
- Return RxMER reads 54 dB when healthy, with no CMTS ceiling.
- Field numbers are readings by eye, so these are ranges, not fits.
- The fixture is one short street. Other plants may need different parameters to land in the same place.
- Rows still use `features::SUMMARY_STRIDE` of 8, so training can't see a 3-subcarrier spike until task 17.

## Implications for hfc-twin

All [inferred]. Under invariant 5 each family above is reached through physical parameters, and the shapes come out of the sim. A field figure is a target to check against, never a lookup.

- **Shape and cause are different labels.** One roll-off can be water, a low-rated part or a one-sided suckout. A spectrum model names the shape, and the graph model and sim verifier name the cause.
- **Labels can come from the physics.** Run the sim with and without the fault on the same seed, and the difference per subcarrier gives the affected band and the drop. A fault with no visible effect is then labelled normal, not given a label it can't show.
- **The model must be allowed to say "don't know".** Both operators keep an "Other".

## Gaps

- **No distributions, no prevalence.** Every number here is an example. Only one real capture is public (`fixtures/field/`), and it's healthy. CableLabs' and Comcast's labelled sets aren't released.
- **Figure readings are by eye.** Fair for ranges, not for fits.
- **One lab modem.** §3's ceiling is a single device.
- **Level against RxMER for a suckout.** No source shows both on one capture. The sim's mapping matches once the modem sits at the 38 dB baseline of the published example, but that's inference.
- **Not read:** [Fox et al.] on water, SCTE 280, SCTE 294, [O'Dell 2023], [Hranac 2023], [Medlock 2023] in detail. SID is NDA.

## Sources

| Short name | Document |
|---|---|
| ZHU2020 | Zhu, Sundaresan, Rupe (CableLabs), Proactive Network Maintenance using Fast, Accurate Anomaly Localization and Classification on 1-D Data Series, IEEE 2020, [arXiv 2007.08752](https://arxiv.org/abs/2007.08752) |
| FERREIRA2020 | Ferreira, Harb et al. (Comcast), [Convolutional Neural Networks for Proactive Network Management](https://www.nctatechnicalpapers.com/Paper/2020/2020-convolutional-neural-networks-for-proactive-network-management), SCTE•ISBE 2020 |
| FERREIRA2023 | Ferreira, Dugan et al. (Comcast), [Detection and Classification of OFDMA Spectrum Impairments by Machine Learning](https://www.nctatechnicalpapers.com/Paper/2023/3579_Ferreira_5204_paper), SCTE 2023 |
| HRANAC2019 | Hranac et al., [Characterizing Network Problems Using DOCSIS 3.1 OFDM RxMER Per Subcarrier Data](https://www.nctatechnicalpapers.com/Paper/2019/2019-characterizing-network-problems-using-docsis-3-1-ofdm-rxmer-per-subcarrier-data), SCTE•ISBE 2019 |
