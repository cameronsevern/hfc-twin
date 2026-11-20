# Downstream PNM measurements (DOCSIS 3.1)

What each downstream measurement is, how a cable modem produces it, its units and thresholds, and how it behaves on healthy and impaired plant.

Values marked "Fig. N" were read off low-resolution raster plots: trust dB to about ±0.1 and probability to about a factor of 2. PNM-3.1 §5 has the same Tables 5 to 7 and Figures 74 to 77 as PNM-HFC §5; differences are cited where they exist. PNM-HFC writes NDI for noise, distortion and interference, and says full band capture (FBC) where PNM-3.1 says wideband spectrum analysis. Fault signatures in depth are in [impairments.md](impairments.md).

## 1. Measurement map

| Measurement | Object [CM-OSSI §] | Requirement | Domain | Sim v1 [inferred] |
|---|---|---|---|---|
| Received power | DsOfdmChannelPower §F.3.2.3.2.32 | PHY §9.3.9 | per 6 MHz band | emit |
| Full band capture | CmSpectrumAnalysis* §D.2.4 | PHY §9.3.2 | log magnitude per bin | emit |
| RxMER per subcarrier | CmDsOfdmRxMer §D.2.8 | PHY §9.3.6 | dB per subcarrier | emit |
| MER margin | CmDsOfdmMerMargin §D.2.9, RequiredQamMer §D.2.11 | PHY §9.3.6.1 | per profile | derivable |
| FEC statistics | ProfileStats, CmDsOfdmFecSummary §D.2.10 | PHY §9.3.7 | per profile, interval | emit |
| Channel estimate | CmDsOfdmChanEstimateCoef §D.2.6 | PHY §9.3.4 | complex per subcarrier | emit |
| NPR | spectral capture of a notch | PHY §9.3.3 | notch depth | derivable |
| Histogram | CmDsHist §D.2.12 | PHY §9.3.8 | ADC code counts, full band | needs nonlinearity |
| Constellation | CmDsConstDispMeas §D.2.7 | PHY §9.3.5 | soft decisions | thin |
| Symbol capture | CmSymbolCapture §D.2.5 | PHY §9.3.1 | I/Q samples | skip |

## 2. OFDM power and level measurement

**Definitions.** SC-QAM digital channel power is average power over the 6 MHz occupied bandwidth, as a thermocouple meter reads it [PNM-HFC §5.1.2]. OFDM power comes in two forms [SCTE 258 §5.2, §6.2]: power per CTA channel, also called OFDM channel power, the average per 6 MHz and not necessarily uniform; and total power over the occupied bandwidth.

    P_total = P_per_cta + 10·log10(N_cta_occupied)      [PHY §7.5.9.1, via PNM-HFC §5.1.3] [normative]

This assumes flat PSD; tilted DAA output is out of scope [SCTE 258 §6.2.1 fn 4]. A CTA channel is occupied if it holds any modulated spectrum or taper region, so occupied bandwidth is a multiple of 6 MHz, 24 to 192 MHz [SCTE 258 §10.3]. Encompassed spectrum runs lowest to highest active subcarrier, at most 190 MHz; modulated spectrum removes excluded subcarriers, at least 22 MHz [SCTE 258 §1.1, §10]. Each band edge has at least 1 MHz of taper [SCTE 258 §6.2.2].

Worked: 42 dBmV per CTA over 192 MHz is 57.05 dBmV; 45 over 24 MHz is 51.02; 38 over two 192 MHz channels is 56.06 [SCTE 258 §7.2]. A lab 96 MHz signal measured 59.68 dBmV total [SCTE 258 §6.2.2] [field], which is 47.64 per CTA [inferred]. A 96 MHz channel with 94 MHz encompassed has 1,880 subcarriers at 50 kHz and 16 CTA channels [PNM-HFC §5.1.7].

**CMTS side.** Required power per channel is `60 − ceil(3.6·log2(N*))` dBmV, where `N* = min(4·Neq', ceil(Neq/4))` if `Neq' < Neq/4`, otherwise `Neq'`, and `Neq = N + 32·N_OFDM` [PHY Table 42, via SCTE 258 §7] [normative]. Modulators hold ±2 dB [DRFI, via SCTE 257 §6.4.1] [normative]. Usual practice matches OFDM PSD to SC-QAM, ignoring pilot spikes; running OFDM hotter risks clipping and compression [PNM-HFC §5.1.4] [SCTE 258 §6.2.1] [guideline]. Pilots are boosted 6 dB but move total power by under 1 dB [PNM-HFC §5.1.8]. The 6 MHz holding the PLC has no excluded subcarriers and the most boosted subcarriers, so it is the highest-power 6 MHz [SCTE 258 §7].

**CM input** [normative]. Total below 40 dBmV over 54 to 1794 MHz; level −15 to +15 dBmV per 6 MHz, equal to −9 to +21 in 24 MHz [PHY, via PNM-HFC §5.1.5]. The usable floor depends on modulation: minimum P6AVG is −6 dBmV for 4096-QAM, −9 for 2048, −12 for 1024 and 512, −15 for 256 and below, with P6AVG (channel power over occupied CTA channels) at most 15 dBmV [PHY §7.5.12.1 Table 46].

**What the CM reports** [normative]. Average power in any 6 MHz band centred at 111 + 6(n−1) MHz, n = 1 to 185, and in the 6 MHz holding the PLC, whose lower edge sits on a 1 MHz grid and can be offset from the CTA grid [PHY §9.3.9]. `ReceivedPower` is TenthdBmV per `BandIndex` 0 to 33: 0 is the PLC band, 1 upward are modulated bands in frequency order, and bands with no active subcarrier are skipped [CM-OSSI §F.3.2.3.2.32 to .34]. Accuracy at the F connector is ±3 dB (tilt within ±1 dB, −12 to +12 dBmV per band) or ±5 dB (tilt +4 to −9 dB, −15 to +15 dBmV) [PHY §9.3.9]. End bands may read low because the channel does not fill those slots; field 96 MHz example: average −3.3, minimum −5.3, maximum −2.7 dBmV [PNM-HFC §5.1.7] [field].

**Instruments.** A spectrum analyzer reads power in its RBW, so add `10·log10(6/RBW_MHz)`: 300 kHz is +13 dB. Taper regions and non-flat signals make a computed total inexact. Per 8 MHz is per 6 MHz + 1.25 dB [PNM-HFC §5.1.6]. Field meters hold ±1.5 to ±2.5 dB, lab instruments about ±1 dB [SCTE 257 §6.3, §6.4.5]; one handheld is ±0.75 dB at 25 °C and ±2.5 dB digital over −18 to +50 °C [SCTE 257 §6.4.9] [field]. Typical per-channel levels in dBmV [SCTE 257 §6.1 Table 1] [field]: node output +38 to +56, amp input +10 to +20, amp output +38 to +60, tap spigot +10 to +20, digital CPE −15 to +15. Coax attenuation moves about 1% per 10 °F [SCTE 257 §6.4.2 fn].

## 3. Full band capture

**Production.** An ADC samples the front end ahead of tuner and demodulator. An optional digital tuner steps segments, a windowed FFT yields I² + Q² per bin, bins are averaged in power, converted to 10·log10, and segments are spliced. The result is magnitude only [PNM-HFC §5.2.2, §5.2.7].

**Control** [CM-OSSI §D.2.4 Table 57] [normative]:

| Attribute | Range | Default |
|---|---|---|
| First / LastSegmentCenterFrequency | Hz | 93 MHz / 993 MHz |
| SegmentFrequencySpan | 1 to 900 MHz | 7.5 MHz |
| NumBinsPerSegment | 2 to 2048 | 256 |
| EquivalentNoiseBandwidth | 50 to 500, hundredths of bin spacing | 150 |
| WindowFunction | other, hann, blackmanHarris, rectangular, hamming, flatTop, gaussian, chebyshev | |
| NumberOfAverages | 1 to 1000 | 1 |

Segments = `(Last − First)/Span + 1` [PNM-HFC §5.2.8.3]. Defaults give 121 segments, 30,976 bins, 29.3 kHz spacing; ENBW 1.5 × spacing = 43.9 kHz, the Hann value [PNM-HFC §5.2.10.1], so a bin reads about 21.35 dB below per-6 MHz level [inferred, arithmetic]. Bin amplitudes are in 0.01 dB; `TotalSegmentPower` is TenthdBmV, summed as linear power [CM-OSSI §D.2.4.2]. File layout is in [collection-management.md](collection-management.md).

**Averaging** is a leaky integrator: `reported = α·accumulated + (1 − α)·current`, `α = 1 − 1/N` [PNM-HFC §5.2.8.7]. Chip vendors differ widely at maximum averaging [PNM-HFC §5.2.4 Fig. 11] [field]. External averaging must be bin by bin in linear power with identical parameters [PNM-HFC §5.2.4].

**Limits** [PNM-HFC §5.2.3]. Dynamic range is typically 30 to 40 dB; beyond it, front-end gain control creates false steps in the floor (examples at 800 and 880 MHz). Capture is slow, several samples per second, so fast bursts are missed, and stitching can add discontinuities. Low-level ingress under a signal is invisible unless it exceeds the signal's peak-to-average ratio. The upstream band shows only through diplexer rejection. Without phase there is no signal quality and no group delay [PNM-HFC §5.2.10.2]. Guard-band ingress may show as a raised floor or spike; whether 30 kHz RBW suffices is unresolved [PNM-HFC §5.2.6.6].

**Calibration.** FBC sits before tuner calibration, so it will not match power MIBs. Compute one offset, FBC minus tuner, averaged in linear power over several channels with matching parameters; example 2.94 dB [PNM-HFC §5.2.5] [field]. The CM must expose a constant relating the two [PHY §9.3.2] [normative].

**Detector defaults**, adjustable, from the CableLabs spectral impairment detector [guideline]:

| Check | Default | Source |
|---|---|---|
| Adjacency delta | 2.9 to 14 dB, 110 to 700 MHz; rejected above channel tilt 1.75 or cumulative tilt 4.0 | [PNM-HFC §5.2.6.3.1] |
| Missing channels | under 68 active; active above 18.0; discard below −35 | [PNM-HFC §5.2.6.4.1] |
| Spectral tilt | absolute value above 15 | [PNM-HFC §5.2.6.13.1] |
| Headend channel match | within 1 dB at same modulation | [PNM-HFC §5.2.6.3] |
| Resonant peak | above 3 dB may impair; field 17 and 18 dB | [PNM-HFC §5.2.6.8] |
| Tilt limit | Europe ±12 dB; North America only the level window | [PNM-HFC §5.2.6.13] |

**Ripple to distance**: `D_ft = 492·Vp/f_MHz`, f being spacing between adjacent peaks or nulls, Vp typically 0.87. It gives echo cavity length, not distance from the modem [PNM-HFC §5.2.6.11]. **Time response without phase**: linear magnitudes in I, zeros in Q, IFFT, drop the image; SC-QAM blocks leave comb teeth every 166.67 ns [PNM-HFC §5.2.10.2]. Water-soaked cable gives non-periodic ripple and a spread-out time response, against a strong single peak for a clean echo [PNM-HFC §5.2.6.12.2]. **Amp misalignment**: compare the floor in the diplex region with the floor above the modulated band; a significant difference suggests a misaligned amp upstream, no numeric limit given [PNM-HFC §8.15] [guideline].

## 4. RxMER per subcarrier

**Definition** [PHY §9.3.6] [normative]. Measured on pilots and PLC preamble symbols, whose values are known, so very low RxMER stays measurable.

    r = y / H_est,  e = r − p,  E_dB = 10·log10(time average of |e|²),  RxMER_dB = S_dB − E_dB

S is average power of the ideal QAM constellation, not the pilot, so no 6 dB correction is needed. Defining case: flat AWGN with data-subcarrier CNR 35 dB gives nominally 35 dB averaged over subcarriers. Scattered pilots rescan every 128 symbols and reach every data subcarrier; E averages tens to hundreds of visits [PNM-HFC §5.3.2], about 0.3 s for 100 visits at a 25 µs symbol [inferred, arithmetic]. RxMER also captures what F-connector SNR cannot: phase noise and clock jitter, intermodulation, CM thermal noise, EMI, digital noise, CM nonlinearity [SCTE 285 §6.1].

**Encoding** [CM-OSSI §D.2.1.3.1, §D.2.8] [normative]. One byte per subcarrier from lowest to highest active, 0 to 63.5 dB in 0.25 dB steps, clipped at 0x00 and 0xFE. 0xFF means no measurement (excluded). One field dump shows taper regions as zeros [PNM-HFC §5.3.1 Fig. 56] [field].

**Accuracy.** There is no absolute requirement [SCTE 285 §7.1 fn 2]. At flat AWGN, 192 MHz, 6 dBmV per 6 MHz, frequency-averaged RxMER repeats within std 0.5 dB, and the 35 dB minus 30 dB delta lies in 4 to 6 dB [PHY §9.3.6] [normative]. CMs track a VSA within about 1 dB until their own limit above about 40 dB [SCTE 285 §7.5] [field]:

| Profile, input SNR | VSA RxMER | PNM RxMER | Throughput [SCTE 285 §7.4] |
|---|---|---|---|
| 4K, 46 | 46.0 | 43.2 | 773 Mbps |
| 4K, 38 | 38.0 | 37.9 | 773 |
| 4K, 35 | 36.4 | 35.6 | 340 |
| 4K, 34 | 36.18 | none | no traffic |
| 1K, 30 | 30.71 | 30.3 | 651 |
| 1K, 29 | none | none | no traffic |

**Healthy shape.** Field 192 MHz, 8K FFT, 7,600 active of 8,192, first active index 296: about 90% of subcarriers in the 42 and 43 dB bins, 5% each in 44 and 41, under 1% below 41 [PNM-HFC §5.3.1 Fig. 56] [field]. The hex values printed in that figure (944 of them) average 41.6 dB with std 0.6 dB [inferred]. Another field 96 MHz channel averages 41.2 dB [PNM-HFC §5.3.1 Fig. 57]. Across a modem population `p2 ≈ 44.03 − 4.72·stddev`, R² 0.75 [PNM-HFC §5.3.5 Fig. 72] [field].

**Reading the plot.** Inverted, the plot is the equalized noise floor at the slicer. Simulated 10 MHz LTE lowers affected subcarriers about 10 dB [PNM-HFC §5.3.4.1]. Edge roll-off depends on taper, cyclic prefix, adjacent-slot energy, total input power and silicon [PNM-HFC §5.3.4.4].

**When ripple appears** [PNM-HFC §5.3.4.2]. Central for the sim. With n1 added before the reflection and n2 after, `e = n1 + n2/H`:

| Noise location | RxMER | Result |
|---|---|---|
| Before reflection | SNR1 | flat, even though spectrum and channel estimate ripple |
| After reflection, such as modem noise figure | \|H\|²·SNR2 | same uninverted ripple as spectrum and channel estimate |
| Both | mix | some ripple; low level can hide it in the floor |

The cavity formula applies: 8 MHz spacing at VF 0.87 is 53.5 ft. The same ripple on modems sharing a path means shared plant; one modem alone on a tap means its drop. Ripple or ingress across shared paths brackets a fault between the last modem showing it and the first not [PNM-HFC §8.9] [field].

**Summary metrics** [CM-OSSI §D.2.8 Table 66] [PNM-HFC §5.3.5] [normative]:

| Attribute | Units | Rule |
|---|---|---|
| Percentile | percent, default 2 | |
| Mean | hundredthDb | mean of dB values over active subcarriers |
| StdDev | hundredthDb | divides by N, on dB values |
| ThresholdRxMerValue | quarterDb | sort ascending, 1-based index floor(N·p/100); N=3677, p=2 gives 73rd |
| ThresholdRxMerHighestFreq | Hz | highest-frequency subcarrier equal to that value |

OSSI says the percentile share is at or below the value; PNM-HFC says 98% lie above it. With ties only the sort rule is exact. Guidance [guideline]: StdDev below 2 dB, 1 dB or less is better [PNM-HFC §5.3.5.3]. For an equal failed p2, prioritize the smaller StdDev, since more subcarriers sit near the failed level [PNM-HFC §5.3.5.6 Fig. 71]. Skewness is not an attribute but a working KPI: StdDev above 1 with skew below −1 flagged ingress (−3.19, −2.26), while micro-reflections had skew above −1 (−0.19, −0.36) [SCTE 285 App. C] [field].

**Thresholds.** These tiers mean different things and must not be merged.

| QAM | PHY min CNR = OSSI ReqMer default | Table 5 average | Table 6 p2 pass | p2 fail below | SCTE 285 AWGN min | SCTE 285 non-AWGN |
|---|---|---|---|---|---|---|
| 16 / 64 / 128 | 15.0 / 21.0 / 24.0 | | | | | |
| 256 | 27.0 | ≥ 29~30 | > 26 | 24 | 24.1 | 26–29 |
| 512 | 30.5 | ≥ 31~33 | > 29 | 26 | 27.1 | 29–32 |
| 1024 | 34.0 | ≥ 34~36 | > 32 | 29 | 30.1 | 32–35 |
| 2048 | 37.0 (37.5 above 1002 MHz) | ≥ 37~39 | > 35 | 32 | 33.1 | 35–38 |
| 4096 | 41.0 (41.5 above 1002 MHz) | ≥ 40~42 | > 38 | 35 | 36.1 | 38–41 |
| 8192 / 16384 | 46.0 / 52.0, OSSI only | | | | 39.1 / 42.1 | 41–44 / 44–47 |

Sources: [PHY §7.5.12.1 Table 46] [normative]; [CM-OSSI §D.2.11 Table 72], quarter-dB values 60, 84, 96, 108, 122, 136, 148, 164, 184, 208; [PNM-HFC §5.3.4.3 Table 5, §5.3.5.6 Table 6] [guideline]; [SCTE 285 App. B Table 11] [guideline].

Table 5 averages carry 5 to 6 dB above threshold; some operators use 3 dB, at more risk [PNM-HFC §5.3.4.3]. For flat RxMER 1024-QAM needs above 30 dB; p2 below 29 dB means uncorrectables begin [PNM-HFC §5.3.5.6]. SCTE 285's AWGN column uses illustrative 3 dB steps; real steps are larger [SCTE 285 App. B fn 4]. Observed profile downgrades came at 37.72 dB from 4K and 30.66 dB from 1K, 1.62 and 0.56 dB above that minimum [SCTE 285 §8.5] [field].

**"Margin" has three meanings.** Guide example: mean RxMER minus threshold, 41.2 − 35 ≈ 6 dB [PNM-HFC §5.3.4.3]. OSSI: `AverageMerMargin = MeasuredAvgMer − ReqAvgMer`, ReqAvgMer being the mean of ReqMer over the profile's non-excluded subcarriers, plus `NumSubcarBelowThrshld` [CM-OSSI §D.2.9] [normative]; required values target CER 1e−5 in AWGN [PHY App. VI, via PNM-HFC §5.12]. physics.md §4.6: p2 minus Table 6 fail value. A 4K link at 41.2 dB has about 6 dB of guide margin and 0.2 dB of OSSI margin [inferred].

## 5. FEC statistics

**Counted** [PHY §9.3.7] [normative], per OFDM channel and per profile:

| Code | Metric | Definition |
|---|---|---|
| Data | Uncorrectables | failed BCH |
| Data | Correctables | failed pre-decoding LDPC syndrome check, then passed BCH |
| Data | Total | all FEC codewords |
| NCP | CRC failures, total fields | failed CRC |
| PLC | Unreliable, total | failed LDPC post-decoding syndrome check |
| MAC | Frame failures, total | failed MAC CRC, frames addressed to this CM, no transition profile |

Up to 4 profiles plus transition profile 5, which counts all codewords, or only T-bit = 1 under codeword tagging [PHY §9.3.7]. Summaries hold 600 one-second and 1,440 one-minute samples per profile; each OSSI entry has timestamp, total, corrected and uncorrectable, 4 bytes each [CM-OSSI §D.2.1.6.3, §D.2.10]. OSSI's illustrative sample shows about 220,000 codewords per second. Counters are long, such as 64-bit; a short-term one-shot takes Ne and Nc limits [PHY §9.3.7]. MIBs: `docsIf31CmDsOfdmProfileStats{TotalCodewords, CorrectedCodewords, UncorrectableCodewords, InFrameCrcFailures}`, `docsIf31CmDsOfdmChan{PlcTotalCodewords, PlcUnreliableCodewords, NcpTotalFields, NcpFieldCrcFailures}` [CM-OSSI Annex]. Listed causes: sweep transmitters, amp overload, laser clipping, impulse noise, loose connections, poor combiner isolation, low SNR [PNM-HFC §5.4.2].

**Why OFDM correctables are high** [PNM-HFC §5.4.5] [guideline]. OFDM counts from the slicer with no decoding benefit, over a 16,200-bit LDPC block; raw data is error free over a block only with margin well over 6 dB [PNM-HFC §5.4.5.3]. SC-QAM counts only after the trellis decoder, on 896-bit RS blocks, so much coding gain is already banked [PNM-HFC §5.4.5.3]. Near 100% correctable on a healthy OFDM link is normal, with under 1e−6 failing to decode, and is common with more than 3 dB of margin [PNM-HFC §5.4.4, §5.4.5]. Correctables indicate health on SC-QAM, not on OFDM [PNM-HFC §5.4.7].

Derived [PNM-HFC §5.4.5.5]: errored before decoding = uncorrectable + correctable; correct after decoding = total − uncorrectable. SC-QAM MIBs give unerrored, correctable, total; uncorrectable = total − unerrored − correctable. Healthy field example: Profile A correctable ratio 1.5e−2, Profile B 3.4e−1, NCP and PLC 0, uncorrectable all 0 [PNM-HFC §5.4.4 Fig. 73] [field]. So the ratio rises with bit loading and need not reach 100% [inferred].

**Priorities** [PNM-HFC §5.4.4] [guideline]: PLC and NCP must show none. Profile A (boot) few or none. Higher profiles may show some, which costs throughput and points at plant limits.

**Measured curves**, RxMER (Es/N0) in dB [PNM-HFC §5.4.5 Figs. 74–77] [field].

| OFDM 256 uncorrectable, dB | 23.45 | 23.8 | 23.9 | 24.0 | 24.15 | 24.3 | 24.4 | 24.5 |
|---|---|---|---|---|---|---|---|---|
| P | 0.48 | 0.10 | 0.02 | 4e−3 | 8e−5 | 3e−6 | 3e−7 | 9e−8 |

| OFDM 4K uncorrectable, dB | 34.5 | 35.0 | 35.2 | 35.3 | 35.4 | 35.5 | 35.6 | 35.8* |
|---|---|---|---|---|---|---|---|---|
| P | 0.29 | 0.05 | 0.016 | 6e−3 | 1e−3 | 8e−5 | 1e−5 | 8e−7 |

| OFDM 256 pre-decoding error, dB | ≤30 | 30.55 | 31.6 | 32.5 | 33.5 | 34.6 | 36.7 | 37.65 |
|---|---|---|---|---|---|---|---|---|
| P | ~1 | 0.66 | 0.20 | 0.04 | 6.5e−3 | 5.5e−4 | 9e−6 | 2e−7 |

| SC-QAM 256 uncorrectable, dB | 25.8 | 26.25 | 26.5 | 27.0 | 27.5 | 28.0 | 28.4 | 28.9 |
|---|---|---|---|---|---|---|---|---|
| P | 0.8 | 0.45 | 0.14 | 0.01 | 1e−3 | 1e−4 | 8e−6 | 8e−7 |

| SC-QAM 256 RS input error, dB | 25.9 | 26.5 | 27.1 | 27.85 | 28.6 | 29.2 | 29.7 | 30.55 |
|---|---|---|---|---|---|---|---|---|
| P | ~1 | 0.45 | 0.12 | 0.01 | 1e−3 | 1.6e−4 | 2e−5 | 8e−7 |

\* Extrapolated; the measured count there was 0, at +15 dBmV input [PNM-HFC Fig. 77 legend]. Fig. 77's caption lists a pre-decoding series but none is plotted, so no measured 4K pre-decoding curve exists. Other orders are unmeasured. SCTE 285 adds that LDPC's error rate flares at high SNR, hence BCH; that 256-QAM OFDM nears quasi-error-free at 24 dB against a 27.0 dB spec; and that SC-QAM 256's usable floor is 28 to 30 dB [SCTE 285 App. A, App. B] [guideline].

**Comparison with physics.md §4.6** [inferred]:

- **Uncorrectable.** The tail fits, but the midpoint does not. Physics has P(m=0) = 0.5; measured is 4e−3 at 256-QAM and about 0.05 at 4K, with the 0.5 point near m = −0.55 at 256. Slope is about 15 per dB at 256 (physics 16), about 10 per dB at 4K.
- **OFDM pre-decoding.** `1/(1+exp(4(m−10)))` runs about 3 dB high at 256-QAM: at m = 8.5 it gives 1.0 against 0.04; at m = 10.6, 0.08 against 5.5e−4. `1/(1+exp(2.2(m−6.8)))` fits within a factor of about 4.
- **Physical alternative.** `P = 1 − exp(−(16200/b)·SER)`, `SER ≈ 4(1−1/√M)·Q(√(3·S/(M−1)))`, b bits per symbol, S linear RxMER. At 256-QAM it gives 0.62, 0.13, 0.018, 1.1e−3 against measured 0.66, 0.20, 0.04, 6.5e−3, then under-reads the tail (2.2e−5 against 5.5e−4 at 34.6 dB). At 4K it gives 0.99 at 41.2 dB, 0.56 at 42.5 dB, 0.08 at 43.74 dB. Physics' oracle is 0.994 at 43.74 dB, about 12 times higher. Not validated at 4K.
- **SC-QAM.** `1/(1+exp(4(m−0.5)))` with threshold 26 is about 0.7 dB pessimistic: 1e−3 at 28.2 against 27.5, 1e−4 at 28.8 against 28.0. `1/(1+exp(5(m−0.2)))` fits. RS input sits +1.0 to +1.6 dB from uncorrectable; physics uses +2.

These curves are flat-RxMER AWGN. FEC acts across subcarriers after interleaving [SCTE 285 App. A], so p2 is a proxy, not a decoder model [inferred].

**PLC** [PNM-HFC §5.10] [PHY §7.5.13]. 400 kHz: 8 subcarriers at 50 kHz or 16 at 25 kHz, 16-QAM, centred in a 6 MHz band with no exclusions and a 1 MHz grid. OCD and Profile 0 DPD repeat at least every 250 ms; no PLC, no D3.1. Guidance [guideline]: level ≥ −15 dBmV, RxMER ≥ 15 dB, usually near channel power and mean RxMER; no unreliable codewords; ignore correctables; place away from ingress, though some operators leave the default.

**NCP** [PNM-HFC §5.10.4 to §5.10.6]. QPSK, 16-QAM or 64-QAM, own bit loading. There should be no CRC-24-D errors; one makes the CM reject all NCP message blocks in that symbol's field.

**MAC CRC** [PNM-HFC §5.11]. Expected 0 or near 0: CMs discard uncorrectable codeword bits at PHY, so `InFrameCrcFailures` does not rise with uncorrectables. Not a health signal.

**Candidate profile** [PNM-HFC §5.12]. OPT-REQ can ask for RxMER per subcarrier, threshold comparison and SNR margin; the CM also reports subcarriers below target minus margin.

## 6. Channel estimate coefficients

One complex value per subcarrier, H(k): the frequency response, whose reciprocals typically become the CM's equalizer [PHY §9.3.4] [normative]. 16-bit two's complement, 2.13 format, **scaled so average power ≈ 1** [CM-OSSI §D.2.6] [normative], so absolute path loss is not carried [inferred]. Measured from pilots with boost corrected; not a spectrum and need not agree with one [PNM-HFC §5.5.5]. SC-QAM taps are time domain and inverse; for a tap view IFFT the coefficients, inverting first to match SC-QAM [PNM-HFC §5.5.3, §5.5.5]. Up to 3,800 or 7,600 coefficients [PNM-HFC §5.5.3].

**Summary attributes** [CM-OSSI §D.2.6 Table 62] [normative]: `AmplitudeRipplePkToPk`, `AmplitudeRippleRms`, `AmplitudeMean` in 0.001 dB; `AmplitudeSlope` in 0.001 dB/MHz; `GroupDelayRipplePkToPk`, `GroupDelayRippleRms`, `GroupDelayMean` in 0.001 ns; `GroupDelaySlope` in 0.001 ns/MHz. GroupDelayRippleRms is not stored in the data file.

**Equations** [PNM-HFC §5.5.7]. Least squares `m = Σ(x−x̄)(y−ȳ)/Σ(x−x̄)²`, `b = ȳ − m·x̄`; residual `r = y − (m·x + b)`; `R_rms = √(Σr²/N)`; `R_pp = max r − min r`. Magnitude: x in MHz, `y = 10·log10(I²+Q²)`. Group delay: unwrap `φ = arg C`, `τ_i = −(φ_i − φ_(i−1))/(2π·Δf)·10⁹` ns, Δf in Hz, giving N−1 values. The mean is implementation dependent and may be removed.

**Healthy reference** [PNM-HFC §5.5.7] [field]. Lab, 543.6 MHz start, 94 MHz, high SNR, some setup slope and ripple:

| Metric | Magnitude | Group delay |
|---|---|---|
| Slope | −0.0128 dB/MHz | −0.0012 ns/MHz |
| RMS ripple | 0.2658 dB | 4.0300 ns |
| Peak-to-peak | 1.2379 dB | 25.4990 ns |

A clean channel still shows about 4 ns RMS group delay ripple; differencing magnifies estimate noise [inferred].

**Impaired lab cases** [PNM-HFC §5.5.5] [field]: 70 dB notch at channel edge; 500 ns echo at −10 dB. For that echo, ideal ripple spacing is 2 MHz, peak-to-peak magnitude 5.69 dB, group delay +120 to −231 ns [inferred]. Fig. 82's residual spans roughly +150 to −400 ns at the low end and +100 to −200 ns at the high end. Fig. 83 shows two tap peaks about 0.5 µs and about −10 dB apart. No alarm limits exist; they are named future work [PNM-HFC §5.5.9].

## 7. NPR

An exclusion band of zero-valued subcarriers, across all profiles, makes a notch; CM spectral or symbol capture measures its depth, showing noise, interference and intermodulation under the signal. It may cause link downtime, so it is for infrequent maintenance [PHY §9.3.3] [normative]. Width is `A = 2B + C`, B taper, C measurement width, 1 to 2 MHz for common RBWs, measured at the centre; about 10 MHz to hunt LTE [PNM-HFC §5.6.4] [guideline]. PHY App. V's minimum exclusion band is 3 MHz [PNM-HFC §5.6.6]. Example: 4K FFT, Nrp 256, C = 2 gives A = 3.95 MHz.

| Nrp | 64 | 128 | 192 | 256 |
|---|---|---|---|---|
| 4K FFT taper (MHz) | 3.575 | 1.845 | 1.325 | 0.975 |
| 8K FFT taper (MHz) | 3.3375 | 1.7125 | 1.1625 | 0.9875 |

[PHY App. V, via PNM-HFC §5.6.4 Table 7]

NPR-like from FBC, new in [PNM-HFC §5.6.5]: compare a gap inside the spectrum with one elsewhere, such as the lower band edge, ensuring taper does not set the floor. An elevated interior floor may mean amp misalignment [guideline]. No numeric thresholds exist.

## 8. Histogram

**Production** [PHY §9.3.8] [normative]. Time-domain samples after the wideband front end, full band, so not tied to one channel. Two-sided, 256 bins (even symmetry) or 255 (odd), typically the ADC's 8 MSBs. 32-bit dwell and hit counts per bin, dwell at least 10 million samples within 30 s, resolving density to 1e−6 with at least 10 hits. Accumulates until restart, disable, near overflow or timeout, default 1800 s [CM-OSSI §D.2.12].

**Interpretation** [PNM-HFC §5.9] [guideline]. Healthy is near Gaussian from many independent subcarriers, with small ADC irregularities at the peak. Metric: `gaussian_delta_dB = 10·log10(gaussian/signal_histogram)`, Gaussian with measured mean and variance, over bins with hits [PNM-HFC §5.9.4.3].

| Case | Peak delta | Figure |
|---|---|---|
| Undistorted lab | below 7 dB, tails included | 112 |
| Low distortion, RxMER 37 dB | about 5 dB | 119 |
| Higher distortion, RxMER 23 dB | about 10 dB | 121 |
| RFoG, severe, barely 16-QAM | above 45 dB; low peak, suppressed tails, bulge | 115 |

Ideal clip at +2σ replaces a tail with a spike (simulated); a real upstream clip showed a softer tail and a spike near 1.25σ [PNM-HFC §5.9.4.1, §5.9.4.2]. Caveats [PNM-HFC §5.9.2]: the histogram sits after a non-standard AGC, which can shift ADC loading mid-capture; a dwell can mix plant conditions; rare pathological shapes stay unexplained. It detects compression, not location [PNM-HFC §5.9.5].

## 9. Constellation and symbol capture

**Constellation.** Equalized soft decisions at the slicer for data subcarriers of one selected constellation; up to 8,192 samples per query [PHY §9.3.5] [normative]. s2.13 complex, average constellation power ≈ 1; `ModulationOrderOffset` 0 to 10, `NumSampleSymbols` default 8192 [CM-OSSI §D.2.7]. Impairments show as spreading, apparent rotation, non-square shape under IQ imbalance, or rounded corners from compression. High orders need zoom on subsets such as a corner 16-symbol cluster [PNM-HFC §5.7.1, §5.7.4]. Matching algorithms are future work [PNM-HFC §5.7.5].

**Symbol capture.** Same symbol at both ends via a PLC trigger message block [PHY §9.3.1]. CMTS: frequency-domain values of the active band, 3,800, 7,600, or 880 (25 kHz, 22 MHz). CM: time-domain samples before the FFT, no guard interval, at 204.8 MHz 4,096 or 8,192, fewer on narrow channels. Uses: channel response at low SNR, nonlinear analysis, noise under signal, ingress location by synchronized capture [PNM-HFC §5.8.3]. No field data yet [PNM-HFC §5.8].

## 10. Behaviour summary

[§N] points to sections of this file. Cells marked † are our own expectation [inferred].

| Plant state | FBC | RxMER | Channel estimate | FEC |
|---|---|---|---|---|
| Healthy | haystacks, floor in gaps, pilots show | mean ~41–43, std ≤ 1 [§4] | small slope, ~0.3 dB RMS, ~4 ns GD [§6] | correctables 1.5% to ~100%, uncorrectable 0 [§5] |
| Echo, noise upstream of it | ripple | flat [§4] | ripple, GD ripple | unchanged † |
| Echo, noise after it | ripple | ripple, same sign [§4] | ripple, GD ripple | follows p2 † |
| In-channel LTE | often invisible [§3] | sharp dip ~10 dB, skew < −1 [§4] | unchanged † | uncorrectable on high profiles if p2 fails [§4] |
| Suckout, notch | notch | dips where noise follows it † | notch | follows p2 † |
| Low level, flat loss | flat drop, compare neighbours [PNM-HFC §5.2.6.14] | falls as modem noise dominates † | none, normalized † | correctables rise, then uncorrectable † |
| Amp overload, laser clip | floor rises in gaps [§3, §7] | 37 → 23 dB in lab [§8] | | uncorrectable [§5] |
| Bursty noise | missed [§3] | time-averaged, understated † | | can error while RxMER looks fine † |

## Implications for hfc-twin

All [inferred].

1. **§4.5, §5.2: split noise by location.** Use `1/MER(f) = Σ_before 1/lin(SNR_i) + Σ_after 1/(|H(f)|²·lin(SNR_i)) + ...`, H(f) being shaping between that source and the modem. A flat `ripple_penalty` cannot give the ripple of PNM-HFC §5.3.4.2 and penalizes echoes the equalizer removes. Reserve an ISI term for echoes beyond `CP − RP`; `CP ≥ RP + CPecho`, and a 2.5 µs CP covers 1,068 ft at VF 0.87 [PNM-HFC §8.13.1].
2. **§4.5: add a modem noise-figure term.** `CNR_cm(f) = L(f) − NF_cm − N_floor(6 MHz)`, after every plant echo. Without it low level or tilt cannot lower or shape RxMER. PHY's 41 dB at −6 dBmV implies NF_cm of about 10 dB or less. Keep `impl_mer_db` as a flat ceiling near 43 to 44 dB.
3. **§4.6: replace curves.** `P_uncorr`: centre near m = −0.55, k 15 at 256, 10 at 4K. `P_err`: the SER form plus a tail floor fit to Fig. 76, or logistic k 2.2, c 6.8 at 256 only; fix the 4K oracle (0.994 → about 0.08 at 43.74 dB). `P_uncorr,SC`: k 5, c 0.2; correctable shift about +1.3 dB. Update Fig. 67 to Fig. 74.
4. **Name thresholds and margins.** Add `rxmer_fail_p2_db` (Table 6) and `req_mer_db` (OSSI defaults) to `profile`. PLC and NCP use data-code curves only as approximation, since their failures are defined differently.
5. **§3.3, `design_levels`.** Add modulation-dependent `L_min` from PHY Table 46: −6 dBmV at 4096-QAM. The fixture's −3.41 dBmV still passes.
6. **§9 `rxmer_db`.** Quantize to 0.25 dB, clip 0 to 63.5, represent excluded as `None`, add `first_active_index`. Add seeded per-subcarrier jitter with σ about 0.6 to 0.7 dB. p2 uses the 1-based sorted index `floor(N·0.02)`; define N < 50. §5.5's oracle fits this, but its "98% exceed" wording does not.
7. **§9 `fbc_dbmv`.** State the reference: per ENBW, not per 6 MHz. Add span, bins, ENBW, averages, a seeded per-modem calibration offset of a few dB, 0.01 dB steps, a 30 to 40 dB dynamic range clamp with AGC steps, pilots at +6 dB, taper regions, diplexer rejection and seeded single-capture scatter. Extend the grid past the 993 MHz default on 1218 plant.
8. **§9 `ds_rx_dbmv`.** One value per channel cannot match an OFDM CM. Use per-CTA bands plus the PLC band at index 0, computed from active subcarriers so edge bands read low and excluded bands drop out. Add seeded error within ±3 dB and a linear-sum `total_input_dbmv`.
9. **§5.5, §9 `ds_chan_est`.** Normalize to average power ≈ 1 in 2.13 range: slope and ripple survive, §3.2 loss does not, and `mag_mean_db` sits near 0. Add estimate noise giving about 4 ns RMS group delay on clean plant, about 9e−4 rad at 50 kHz if the lab used 50 kHz. Do not diagnose on `gd_mean_ns`.
10. **§9 `codewords`.** Add the transition profile. Total counts all codewords on the profile, not only those addressed to the modem [PNM-HFC §5.4.2, §5.11]. Derive rolling windows rather than store them. Bursty faults need FEC evaluated on peak noise while RxMER uses time-average power.
11. **Missing from §9, derivable now:** `rxmer_p2_highest_freq_hz`, `rxmer_skew` (derived, not in the MIB), per-profile `{measured_avg_mer_db, req_avg_mer_db, avg_margin_db, n_below_thresh}`, per-band levels, `total_input_dbmv`, `plc_rxmer_db`, `plc_level_dbmv`, `mac_frames{total, crc_failures}` (near zero), `TotalSegmentPower`. Add capability bits per `docs/tasks/01-schemas.md`.
12. **NPR.** Derive from `L(f)` and the cascade noise plus CIN floor in gaps (§4.3, §4.4), via FBC. No new field unless `channel_plan` models an exclusion band.
13. **Defer** histogram and symbol capture: histogram needs a compression or clipping fault that §8 lacks. A constellation generated as a Gaussian cloud from RxMER adds no information.
14. **Oracles:** 42 dBmV over 192 MHz → 57.05; 3,677 subcarriers → index 73; −10 dB, 500 ns echo → 5.69 dB p-p, 2 MHz; 8 MHz, VF 0.87 → 53.5 ft; Fig. 76 points; default ENBW offset 21.35 dB.

## Gaps

- **4K pre-decoding curve** is missing from Fig. 77, so the OFDM correctable rate at 4K rests on our inferred formula. Orders other than 256 and 4K have no measured curves.
- **Non-flat RxMER FEC.** No source gives an effective-RxMER mapping. PNM-3.1 §5.3.4.3.1 only names averaging over dB, noise, inverse noise or BER; PNM-HFC drops that note.
- **Unclear text.** PNM-HFC §5.4.4 calls uncorrectables as high as 8 dB above FEC threshold the primary indicator; meaning unclear, not used. PNM-HFC §5.8 says symbol capture is implemented in CMs only in reference to CMTS platforms; unclear.
- **Source slips.** PNM-HFC §5.1.6 computes 48 dBmV per 6 MHz, then uses 38 for total (same in PNM-3.1). PNM-HFC §5.2.5's Table 1 averages about −0.01 dBmV, text says −0.1 and subtracts 0.1 to reach 2.94. PNM-HFC §5.3.4.3 cites Fig. 52 for the 41.2 dB example; it is Fig. 57. PNM-HFC §5.5.3 prints 190 kHz, meaning 190 MHz. SCTE 285 §8.5 cites Table 3 for minimums, actually Table 11. SCTE 285 Table 6 is non-monotonic at tests 4 and 5; not used.
- **Fig. 56.** Its histogram bins (42 to 43 dB) read higher than its printed hex values (mean 41.6 dB); bin labelling is unclear, and only 944 of 7,600 values are printed.
- **Modem noise figure** is cited only to SCTE 293-11, unread; NF_cm is inferred from PHY Table 46. Subcarrier spacing of the §5.5.7 lab data is not stated.
- **FBC bin statistics** on OFDM (unaveraged scatter) are not quantified in any source read.
- **Fig. 83** peak values were a rough read of a low-resolution image.
- **File layouts** are only sampled here; SC-QAM OSSI power accuracy and SC-QAM MER reporting were not read in detail.
