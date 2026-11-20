# DOCSIS 3.1 physical layer

What the sim, the profile schema and the channel plan need from the D3.1 PHY: band plans, OFDM and OFDMA numerology, modulation, FEC, levels with their reference bandwidths, performance limits, plant assumptions and profile management.

Frequencies are in MHz unless a subcarrier spacing is quoted in kHz, levels are in dBmV, and everything is 75 Ω. Sources already use these units. Conversions we did are marked. Tables 3 and 4 of the PHY state what the spec assumes plant looks like, not what plant must do, so they are tagged [guideline] here.

## 1. Reference bandwidths

| Quantity | Reference | Definition |
|---|---|---|
| Downstream level, `P6AVG` | 6 MHz (one CEA channel) | Measured channel power ÷ number of occupied CEA channels [PHY §7.5.12.1] |
| Downstream level range | 24 MHz in the table, with the 6 MHz equivalent beside it | −9 to +21 dBmV/24 MHz is −15 to +15 dBmV/6 MHz; the offset is 6.02 dB [PHY §7.5.11] |
| CM upstream reported power, `P1.6r_n` | 1.6 MHz | Channel power when fully granted, normalized to 1.6 MHz; 1.6 MHz is four minislots, or 64 subcarriers at 25 kHz or 32 at 50 kHz [PHY §7.4.12, §7.4.12.2.1] |
| CM total transmit power | Whole transmit channel set (TCS) | Sum over channels bursting at that moment; `Pmax` is the capability [PHY §7.4.12.2, §7.4.12.2.1] |
| CMTS receive set point | 6.4 MHz | Intended received power normalized to 6.4 MHz [PHY §7.4.14.1] |
| Plant CNR and noise | Occupied bandwidth | Total signal ÷ total noise in the occupied bandwidth [PHY §7.5.12.1 Table 46 note 1, §7.4.14.2 Table 18 note 1] |

An SC-QAM channel 6.4 MHz wide carries P1.6r + 6 dB [PHY §7.4.12.2]. A 6.4 MHz reference therefore reads 6.02 dB above a 1.6 MHz reference for a flat signal [inferred].

Subcarrier power, downstream: 6 MHz holds 240 subcarriers at 25 kHz and 120 at 50 kHz, so a data subcarrier sits about 23.8 dB or 20.8 dB below the 6 MHz level [inferred]. Pilots carry extra power (§6), which moves data subcarriers another 0.2 to 0.3 dB lower on a full 190 MHz channel with 56 continuous pilots [inferred, our arithmetic].

Subcarrier power, upstream, is stated directly [normative, PHY §7.4.12.3]:

    Pt_sc_i = P1.6r_n − P1.6delta_n + Pre-Eq_i − 10·log10(32 or 64)

`P1.6delta_n` is 0 dB without boosted pilots, 0.5 dB with them at 25 kHz, 1 dB with them at 50 kHz.

## 2. Band plans and splits

**Upstream upper band edges** are 42, 65, 85, 117 and 204 MHz, all starting at 5 MHz; a CM must support at least one edge of 85 MHz or more, and be configurable among those it supports [normative, PHY §7.2.3]. The CM modulator must cover 5 to at least 204 MHz with agile channel placement, and the CM may go above 204 MHz [normative, PHY §7.2.3]. A CMTS must cover the same range [normative, PHY §7.2.4].

**Downstream edges** [normative, PHY §7.2.1, §7.2.2, §7.5.11]:

| Edge | CM | CMTS |
|---|---|---|
| Lower, 258 MHz | MUST | MUST |
| Lower, 108 MHz | SHOULD, only when the upstream edge is 85 MHz or less | SHOULD |
| Upper | MUST support one or more of 1002, 1218, 1794 MHz; demodulator MUST reach at least 1218 MHz | MUST 1218, MAY 1794 |

The plant is also assumed to have lower edges of 54 or 87.5 MHz [guideline, PHY §5.2.1.1]. Downstream always sits above upstream, and there is no frequency inversion in either direction [normative, PHY §7.2, §5.2.4].

| Split | Upstream | Downstream used here | Note |
|---|---|---|---|
| Sub-split | 5–42 | 54–1002 or 1218 | Usable upstream in practice starts near 15 MHz, leaving about 27 MHz [field, TR-MIDSPLIT §1.2] |
| Mid-split | 5–85 | 108–1218 | 80 MHz assumed usable for D3.1, or up to 12 × 6.4 MHz (76.8 MHz) of D3.0 [guideline, TR-MIDSPLIT §5] |
| High-split | 5–204 | 258–1218 | CM must transmit 192 MHz of active channels at this edge [normative, PHY §7.2.3] |

The PHY lists 117 MHz but gives it no lower downstream edge of its own; since 108 MHz is tied to 85 MHz or less, a 117 MHz plan falls to 258 MHz [inferred].

**Channel counts.** CM and CMTS each support at least 2 OFDM channels of up to 192 MHz, at least 2 OFDMA channels of up to 95 MHz, at least 32 downstream SC-QAM and at least 8 upstream SC-QAM channels [normative, PHY §7.2.1–§7.2.4, §6.2.1, §6.4.2].

An OFDMA channel encompasses at most 95 MHz but may occupy up to 96 MHz [PHY §7.2.5.3; TR-MIDSPLIT §3], so the 192 MHz of active channels at the 204 MHz edge most likely counts two channels by occupied bandwidth [inferred; see Gaps].

## 3. Migrating a split

Sources cover the return laser and legacy devices. They do not cover amp and node diplexers.

**Return laser loading.** At constant PSD, moving from 42 to 85 MHz raises average laser input by `10·log10(BW_after / BW_before)`, counting occupied bandwidth, not band width. To hold laser input constant, PSD falls by the same amount: 4 to 10 × 6.4 MHz channels is 3.98 dB, which costs about 1 bit/s/Hz [guideline, TR-MIDSPLIT §5.1]. Their example is 4 × 6.4 MHz at 12 dBmV each: 18.02 dBmV total, −56.06 dBmV/Hz, falling to −60.04 dBmV/Hz.

**PAPR.** An 80 MHz OFDMA signal exceeds a 4 dB instantaneous PAPR about 8.4% of the time, largely regardless of the modulation mix [field, TR-MIDSPLIT §5.2]. Hard clipping gives an SNDR estimate; a 4 dB clipping ratio on a 40 dB signal gives 36.6 dB [guideline, TR-MIDSPLIT §5.3].

**Laser operating window** [field, TR-MIDSPLIT §6.1.1], measured NPR across 16 analog and 6 digital lasers:

| Laser | Max total input (linear) | Min for 1024-QAM (35.5 dB) | 3 dB headroom | 6 dB headroom | Dynamic range |
|---|---|---|---|---|---|
| Analog DFB | 27 dBmV | 7 dBmV | 10 dBmV | 13 dBmV | 20 dB |
| Digital return | 25 dBmV | 11 dBmV | 13 dBmV | not reached; 40.5 dB ceiling (test gear limit) from 16 dBmV | 14 dB |

3 to 6 dB of headroom above the QAM's CNR is typical [guideline, TR-MIDSPLIT §6.1]. 2048-QAM adds 4 dB to the analog minimum and 4096-QAM adds 7 dB, leaving 16 and 13 dB of range [field, TR-MIDSPLIT §6.1.1.1].

Their worked budget targets 1024-QAM: 36 dB needed at the CMTS, 1 dB noise margin, 38 dB at the node. This gives 10 to 22.6 dBmV average input for analog lasers and 13 to 20.6 dBmV for digital [guideline, TR-MIDSPLIT §6.1.2]. Digital return dynamic range is set by the A/D, not the laser [guideline, TR-MIDSPLIT §6.1.1.2].

**Legacy devices.** Tuners receive at 54 to 108 MHz, so CM transmit at 54 to 85 MHz can overload them [guideline, TR-MIDSPLIT §6.2]. In-home CM-to-STB isolation measured 9 to 25 dB: up to 46 dBmV/6.4 MHz is safe at the best case, and about 30 dBmV/6.4 MHz is the more likely limit [field, §6.2.1]. Neighbours are not a concern unless a tap is faulty: same-tap port isolation 32 dB (14 dB 2-port) and 29.7 dB (11 dB 8-port EOL), above 38 dB house to house; next tap 51 dB (14 dB 2-port, 8 dB 4-port), above 81 dB house to house [field, §6.2.2, §6.2.3].

Fixes for in-home isolation are better splitters or inline filters on legacy devices [guideline, TR-MIDSPLIT Executive Summary]. D3.0 CMs filling 5–85 MHz also limit QAM: aggregate spurious of −33, −35 and −38 dBc at 12, 8 and 4 channels, so 1024-QAM coexists only if legacy occupancy stays at 4 channels [field, TR-MIDSPLIT §6.1.1.1].

**Plant consequences** [inferred, no source]: diplexers in amps, nodes and modems must move together. Upstream loss rises with √f, so the 85 MHz end needs about 1.42 × the 42 MHz cable loss in dB and the 204 MHz end about 2.2 ×. Lower downstream edges moving from 54 to 108 to 258 MHz remove spectrum and shift tilt calculations. The DRW's allowance for 10 dB of upstream tilt (§10) is a limit to compare against.

## 4. Numerology

Sampling clocks derive from a 10.24 MHz master clock [normative, PHY §7.3.1, §7.3.3]. FFT size maps to spacing differently in each direction because the sample rate differs [normative, PHY §7.3.1 Table 6, §7.3.2 Table 7, §7.2.5.1, §7.2.5.3, §7.4.13 Table 16].

| Parameter | Downstream 4K | Downstream 8K | Upstream 2K | Upstream 4K |
|---|---|---|---|---|
| Sample rate | 204.8 MHz | 204.8 MHz | 102.4 MHz | 102.4 MHz |
| IDFT size | 4096 | 8192 | 2048 | 4096 |
| Subcarrier spacing | 50 kHz | 25 kHz | 50 kHz | 25 kHz |
| Useful symbol T_U | 20 µs | 40 µs | 20 µs | 40 µs |
| Max active subcarriers | 3800 | 7600 | 1900 | 3800 |
| Max encompassed spectrum | 190 MHz | 190 MHz | 95 MHz | 95 MHz |
| Allowed index k | 148–3947 | 296–7895 | 74–1973 | 148–3947 |
| Channel width | 24–192 MHz | 24–192 MHz | 10–95 MHz | 6.4–95 MHz |

Narrower channels keep the spacing and zero more subcarriers [normative, PHY §7.3.1]. Index 0 is the lowest frequency; subcarriers number from low to high across the whole FFT span including excluded ones [normative, PHY §7.5.7.1, §8.2.2, §8.3.3]. Every subcarrier centre is an integer multiple of the spacing [normative, PHY §7.3.3, §7.4.2.1]. The downstream X(0) frequency is sent via the PLC as a 32-bit value in Hz, along with N [normative, PHY §7.5.13.4]. So:

    f_k = f_0 + k · Δf

**Downstream bandwidth terms** [normative, PHY §7.5.9]: encompassed spectrum is highest minus lowest modulated subcarrier centre, plus one spacing. Modulated spectrum is encompassed minus internal exclusions. Occupied bandwidth is a whole number of 6 MHz CEA channels, at least 24 MHz, covering the modulated spectrum and its taper. For example, 189.7 MHz encompassed can occupy 192 MHz.

**Adjacency rules** [normative, PHY §7.2.5.1]: at least 1 MHz between an SC-QAM channel edge and the nearest OFDM subcarrier centre; at least 2 MHz between active subcarriers of adjacent asynchronous OFDM channels; none if they share FFT length, CP, time, frequency and phase, such as two 8K channels built from one 16K FFT at 409.6 MHz.

## 5. Cyclic prefix, roll-off and echoes

Options by direction [normative, PHY §7.5.8.1 Tables 39–40, §7.4.10 Tables 9–10]:

| Direction | CP options (µs, samples) | Roll-off options (µs, samples) |
|---|---|---|
| Downstream, at 204.8 MHz, both FFT sizes | 0.9375/192, 1.25/256, 2.5/512, 3.75/768, 5.0/1024 | 0/0, 0.3125/64, 0.625/128, 0.9375/192, 1.25/256 |
| Upstream, at 102.4 MHz | 0.9375/96, 1.25/128, 1.5625/160, 1.875/192, 2.1875/224, 2.5/256, 2.8125/288, 3.125/320, 3.75/384, 5.0/512, 6.25/640 | 0/0, 0.3125/32, 0.625/64, 0.9375/96, 1.25/128, 1.5625/160, 1.875/192, 2.1875/224 |

Roll-off must be below CP, except upstream initial ranging [normative, PHY §7.5.8.1, §7.4.10]. A roll-off of 0 downstream is for evaluation only [normative, PHY §7.5.8.1]. Windowing is a Tukey raised cosine embedded in the CP, so symbol duration is `T_U + T_CP` [normative, PHY §7.3.1, §7.3.3].

Taper width from the band edge for each roll-off, informative [guideline, PHY Appendix V Table 55]: 4K: 64 samples 3.575 MHz, 128 1.875, 192 1.325, 256 0.975. 8K: 64 3.3375, 128 1.7125, 192 1.1625, 256 0.9875.

**An echo inside the CP causes no ISI or ICI.** Beyond it [PHY §7.4.10]:

    ISI = (τ − T_CP) · A² / T_U

τ is echo delay, A its relative amplitude, T_U 20 or 40 µs. ICI is of the same order. The formula appears in the upstream section; downstream shares the procedure by reference [PHY §7.5.8.1].

At v_f 0.87 (physics.md §1.3), one-way echo distance covered by CP is about 428 ft/µs [inferred]: 0.9375 µs is 401 ft, 2.5 µs is 1,070 ft, 5 µs is 2,139 ft, 6.25 µs is 2,674 ft.

## 6. Subcarrier accounting

### Downstream classes

Each OFDM symbol partitions as `N = N_C + N_S + N_E + N_P + N_D` [normative, PHY §7.5.5.5]: continuous pilots, scattered pilots, excluded, PLC, data. `N_C`, `N_E` and `N_P` are fixed for a channel config. `N_S` and `N_D` vary by symbol but their sum `N_I` does not. `N_I` is what gets interleaved, at most 7537 (8K) or 3745 (4K) [normative, PHY §7.5.6].

### Exclusions

Downstream [normative, PHY §7.2.5.2]:

- At least one contiguous modulated band of 22 MHz or more; each contiguous band at least 2 MHz.
- Exclusion bands at least 1 MHz, growing by one subcarrier. FCC aeronautical exceptions may be narrower: 121.400–121.600, 156.750–156.850, 242.950–243.050, 405.925–406.176 MHz.
- Exclusion bands and excluded subcarriers total 20% or less of spanned modulated spectrum.
- Individually excluded subcarriers: 5% or less of any contiguous band, 5% or less in any 6 MHz moving window, 20% or less in any 1 MHz moving window.
- The 6 MHz around the PLC has none.
- All are common to every profile.

Upstream: subcarriers outside the encompassed spectrum are excluded, and excluded or unused subcarriers cannot sit inside a minislot, but may lie anywhere between minislots [normative, PHY §7.2.5.4]. Exclusions apply channel-wide, independent of profile [normative, PHY §8.2.3.1].

### PLC

Every OFDM channel has one. It sits at the centre of a 6 MHz clean window whose lowest subcarrier falls on a whole-MHz frequency, not necessarily in the largest band, and preferably in quieter spectrum [normative, PHY §7.5.13.1, §7.5.13.2].

4K has 8 PLC subcarriers in a 56 + 8 + 56 window, with predefined continuous pilots 15, 24, 35, 47 subcarriers from each PLC edge. 8K has 16 in 112 + 16 + 112, with pilots at 30, 48, 70, 94 [normative, PHY §7.5.13.2 Table 47, §7.5.15.2.1 Table 50].

**PLC modulation is fixed at 16-QAM.** It is not configurable [normative, PHY §7.5.13.2, §7.5.13.9]. The preamble is BPSK; the frame is 128 symbols, 8 of preamble and 120 of data [normative, PHY §7.5.13.2, §7.5.13.3]. FEC is a (384,288) punctured LDPC [normative, PHY §7.5.13.6]. It is interleaved on its own, not with data [normative, PHY §7.5.13.2]. It carries N, X(0) frequency, PLC location L, continuous pilot locations, excluded subcarriers, bit loading, CP, roll-off, time interleaver depth and NCP modulation [normative, PHY §7.5.13.4].

### NCP

Selectable as QPSK, 16-QAM or 64-QAM, signalled by the PLC, constant across the channel [normative, PHY §7.5.5.5.2, §7.5.14.3]. Each message block is 24 bits, LDPC (48,24) at rate 1/2; the last in each symbol carries a CRC-24-D over the rest [normative, PHY §7.5.14.1, §7.5.14.2]. That is 24, 12 or 8 subcarriers per block for QPSK, 16-QAM or 64-QAM [normative, PHY §7.5.14.3]. NCP is placed from the highest active data subcarrier downward, while data fills upward [normative, PHY §7.5.14.4]. It has its own zero-bit-loading profile, ID 255 [normative, PHY §7.5.5.5.2; MULPI §6.4.41]. It shares data's randomizer and interleavers [normative, PHY §7.5.14.5].

### Downstream pilots

Both types are BPSK, boosted to twice the RMS amplitude of other subcarriers, about 6 dB [normative, PHY §7.5.15.3, §7.5.15.4].

- **Scattered:** one every 128 subcarriers in each symbol, in a pattern repeating every 128 symbols. Synchronized to the PLC preamble. 4K shifts one subcarrier per symbol. 8K shifts two per symbol, with a one-subcarrier offset for the second 64 symbols, so every location is visited [normative, PHY §7.5.15.1]. Zero-valued in exclusion zones, and not sent in the PLC band.
- **Continuous:** 8 predefined around the PLC, plus a count set via the PLC of `min(max(8, ceil(M·(F_max − F_min)/190 MHz)), 120)`, with 48 ≤ M ≤ 120 and 48 typical. Total 16 to 128. Placement is near uniform, moved at least 1 MHz from spectral edges, and at least one in each isolated band [normative, PHY §7.5.15.2.2].

Scattered pilots move across the band, and that is how RxMER reaches every active subcarrier [normative, PHY §9.3.6].

### Upstream minislots and pilots

Minislots hold 8 subcarriers at 50 kHz and 16 at 25 kHz. Symbols per frame K starts at 6; its maximum is 18, 24 or 36 at 50 kHz and 9, 12 or 18 at 25 kHz, for BW ≥ 72, 48 to 72, and < 48 MHz [normative, PHY §8.2.3.1 Table 51, §8.2.3.2].

A minislot is 400 kHz wide either way [inferred]. Pilots are BPSK; complementary pilots carry data at max(M−4, 1) bits [normative, PHY §7.4.16]. There are seven patterns per minislot size plus two for subslots: 1–7 for 8 subcarriers, 8–14 for 16. CMs support all seven. CMTSs must support 1–4 or 8–11 and should support the rest; one channel can't mix the two groups [normative, PHY §7.4.16.1, §7.4.16.2]. Pattern 1 is the most efficient and pattern 4 more robust [guideline, TR-PMA §8.2.4]. Patterns 5–7 and 12–14 boost pilots by a factor of 3 in power, about 4.7 dB [normative, PHY §7.4.16.3].

## 7. Modulation and bit loading

**Downstream data** uses 16, 64, 128, 256, 512, 1024, 2048 and 4096-QAM as mandatory for CM and CMTS; 8192 and 16384-QAM are optional [normative, PHY §7.5.5.1]. Per-subcarrier bit loading takes values `A_i(k) ∈ {0, 4, 6, 7, 8, 9, 10, 11, 12, 13, 14}`, where 0 means zero-bit-loaded [normative, PHY §7.5.5.5.1]. Zero-bit-loaded subcarriers are still transmitted, as BPSK at the same power as others [normative, PHY §7.5.5.5]. Table 45 lists QPSK as a CM modulation, but data bit loading excludes it, and FEC section text says it is not required; QPSK is for the NCP [inferred, PHY §7.5.11, §7.5.4, §7.5.5.5.1].

**Upstream data** uses QPSK to 4096-QAM at bit loads 2–12. CMs must support all. CMTSs must support up to 1024-QAM and should support 2048 and 4096 [normative, PHY §7.4.7.1, §8.2.3.1 Table 51]. BPSK is for pilots only [normative, PHY §7.4.7.1]. Bit loading is constant within a minislot and can vary across minislots [normative, PHY §8.2.3.3].

**A profile** [normative, PHY §7.5.5.5, §8.3.1, §8.2.1]: downstream, one constellation per subcarrier, chosen for a group of CMs with similar SNR. Upstream, bit loading and pilot pattern per minislot, mapped to a data IUC. Excluded subcarriers, continuous pilots and PLC are defined separately and override the bit loading.

**Mixed modulation** is when a profile uses more than one non-zero order across the symbol, giving about 1.5 dB steps when alternating adjacent orders. It sets how FEC bit interleaving runs for the whole profile, even on codewords that happen to land on one order. Zero-bit-loaded subcarriers don't count [normative, PHY §7.5.4.1.1].

Profile IDs: downstream 0–15, up to 16 per channel; 0 is Profile A, the boot profile, and 1–3 are B–D; ID 255 is the NCP profile [normative, MULPI §6.4.41; PHY §8.3.1]. Upstream data IUCs are 5, 6, 9, 10, 11, 12, 13; IUC 13 is required [normative, MULPI §6.4.3.4]. Minimum support is 4 downstream profiles plus a transition profile per OFDM channel on the CM, and 4 per CM on the CMTS, which should offer 16 on a 192 MHz channel [normative, MULPI §7.12.1].

## 8. FEC

**Downstream** is concatenated BCH and LDPC, based on DVB-C2's short frame, rate 8/9 only [normative, PHY §7.5.4.2 Table 23].

A full codeword is 16,200 bits (2025 bytes): 1,800 LDPC parity, then 14,400 LDPC input, which is 168 BCH parity plus 14,232 BCH input, which is a 2-byte header plus at most 1777 bytes of DOCSIS payload [normative, PHY §7.5.4.2, §8.3.2].

Shortening removes payload only; parity and header stay [normative, PHY §7.5.4.2.3, §8.3.2]. Cells per full codeword = 16200 / bit load: 16-QAM 4050, 64 2700, 128 2315, 256 2025, 512 1800, 1024 1620, 2048 1473, 4096 1350, 8192 1247, 16384 1158 [normative, PHY §7.5.5 Table 31]. The table rounds non-integer results up [inferred]. Shortened and mixed-modulation codewords pad with zero bits to fill the last symbol [normative, PHY §7.5.5].

**Upstream** uses three quasi-cyclic LDPC codes with no BCH [normative, PHY §7.4.3 Table 8]:

| Code | Rate | N | K | Parity | Shortened instead of stepping down at |
|---|---|---|---|---|---|
| Long | 8/9 | 16,200 | 14,400 | 1,800 | 11,881 or more bits left |
| Medium | 28/33 (about 85%) | 5,940 | 5,040 | 900 | 3,421 or more bits left |
| Short | 3/4 | 1,120 | 840 | 280 | 281 or more bits left; needs 420 info bits, borrowed from the prior codeword if short |

Thresholds are from [normative, PHY §7.4.3.1.1]. A grant under 700 bits is ignored, and the CM doesn't transmit in it [normative, PHY §7.4.3.1.1]. Codewords can cross minislot and frame boundaries [normative, PHY §8.2.1].

**Counters** [normative, PHY §9.3.7]: uncorrectable means failed BCH. Correctable means failed the pre-decoding LDPC syndrome check and passed BCH. NCP counts CRC failures and total fields. PLC counts codewords failing the post-decoding syndrome check, plus total.

**Interleaving, only as it affects the sim:** time interleaver depth M is 1–32 at 20 µs symbols and 1–16 at 40 µs [normative, PHY §7.5.6.1]. Time interleaving spreads burst damage, and frequency interleaving spreads narrowband ingress, across many codewords [normative, PHY §7.5.6]. So a few bad subcarriers cost every codeword a little instead of a few codewords a lot [inferred].

## 9. Downstream level requirements

**CM input** [normative, PHY §7.5.11 Table 45]:

| Parameter | Value |
|---|---|
| Total input power | under 40 dBmV, 54–1794 MHz, assuming negligible power outside |
| Level range | −9 to +21 dBmV/24 MHz, equal to −15 to +15 dBmV/6 MHz |
| Max PSD, other demodulated bandwidth | at most min[X − 10·log10(24) + 10, 21 − 10·log10(24)] dBmV/MHz |
| Max PSD, non-demodulated, first 12 MHz occupied | at most min[X − 10·log10(24) + 10, 26 − 10·log10(24)] dBmV/MHz |
| Max PSD, non-demodulated, remaining bandwidth | at most min[X − 10·log10(24) + 10, 21 − 10·log10(24)] dBmV/MHz |
| Input impedance, return loss | 75 Ω, above 6 dB across the supported band |

X is the average power, in dBmV per 24 MHz, of the lowest-power 24 MHz of modulated spectrum being demodulated. The term `X − 10·log10(24) + 10` means no other spectrum may exceed that weakest 24 MHz by more than 10 dB in PSD [inferred, our reading]. The absolute caps equal 15 dBmV/6 MHz (demodulated) and 20 dBmV/6 MHz (up to 12 MHz of non-demodulated) [inferred]. The spec doesn't use the word tilt, but this limits band-wide spread at the CM input.

The spec notes that level range implies nothing about BER, which is covered separately [normative, PHY §7.5.11]. The usable floor rises with modulation (§11).

The CM is assumed to sit at the drop-home boundary or behind the first 2-way splitter [guideline, PHY §5.1]. Maximum analog carrier at its input is 17 dBmV PEP [guideline, PHY §5.2.2.1 Table 3].

**CMTS output** [normative, PHY §7.5.9, §7.5.9.1.1 Table 42]: required power per channel is `60 − ceil(3.6·log2(N*)) dBmV`, with 8 dB of adjustment and ±2 dB accuracy, and each OFDM channel counts as 32 equivalent channels. Adjacent channels stay within 0.5 dB, and any two within 2 dB, across 108–1218 MHz. Total power = per-CEA power + 10·log10(occupied CEA channels). This is headend output, not node output [inferred]; levels in physics.md §3 come from elsewhere.

## 10. Upstream level requirements

**CM transmit** [normative, PHY §7.4.12, §7.4.12.2, §7.4.13]:

- `Pmax` is the CM's total capability: at least 65 dBmV, which is also the default; higher is allowed.
- `Neq = ceil(BW_legacy/1.6) + ceil(BW_OFDMA/1.6)`, using occupied bandwidth (terminology §7.4.12.3.1 omits the first ceil).
- **`P1.6hi = Pmax − 10·log10(Neq)`**, a single top value for all channels in the TCS.
- If modulated spectrum is 24 MHz or less, the CMTS must keep commanded P1.6hi at or below `53.2 + (Pmax − 65)`. That is a PSD of Pmax per 24 MHz.
- `P1.6min = 17 dBmV`. Bottom limit `P1.6low_n` equals this for SC-QAM and non-boosted OFDMA; +0.5 dB (25 kHz) or +1 dB (50 kHz) with boosted pilots; S-CDMA has its own form. Initial ranging may go 9 dB lower per subcarrier.
- Reporting accuracy ±2 dB with pre-eq off.

So **53.2 dBmV/1.6 MHz applies only to a TCS of 24 MHz or less**. It is the same value as 65 − 10·log10(15). For wider sets the top falls [inferred, computed at Pmax = 65]:

| TCS bandwidth | Neq | P1.6hi (dBmV/1.6 MHz) |
|---|---|---|
| 24 MHz | 15 | 53.2 |
| 48 MHz | 30 | 50.2 |
| 80 MHz (mid-split) | 50 | 48.0 |
| 96 MHz | 60 | 47.2 |
| 192 MHz (high-split) | 120 | 44.2 |

The Dynamic Range Window [normative, PHY §7.4.12.2.1]:

    P1.6hi − (P1.6load_min_set + 12 dB) ≤ P1.6r_n ≤ P1.6hi − P1.6load_min_set

`P1.6load_min_set` is commanded by the CMTS. The CMTS normally keeps a 12 dB DRW, which handles up to 10 dB of plant tilt across the upstream band; pre-eq also compensates for tilt [normative, PHY §7.4.12.2]. The CMTS should not command a CM outside the DRW except under MSM. If it does, CM fidelity requirements stop applying and the CM flags an error in RNG-REQ [normative, PHY §7.4.12.2].

**When a CM runs short of power**, the CMTS may shrink the TCS, assign a lower-modulation profile, or use Maximum Scheduled Minislots: commanding higher PSD than a 100% grant could support and limiting concurrent minislots. MSM isn't signalled to the CM, which should degrade gracefully [normative, PHY §7.4.12.1].

**CMTS receive** [normative, PHY §7.4.14.1 Table 17, checked on PDF p. 87]: operates with total input up to 31 dBmV including noise and ingress. The maximum set point is 10 dBmV/6.4 MHz for every order; minimum set points are in the §11 table. Bursts may land −9/+3 dB around the set point for orders to 256-QAM and −3/+3 dB for 512-QAM and above.

So 0 dBmV/6.4 MHz serves 1024-QAM but not 2048 or 4096 [inferred]. Probe testing cites +5.4 dBmV ±1 dB on 22 MHz as about 0 dBmV/6.4 MHz [normative, PHY §7.4.17].

Reverse gain varies seasonally and diurnally up to 14 dB min to max [guideline, PHY §5.2.2.2 Table 4]. Uniform PSD loading is the common practice, with power as low as the margin allows [guideline, PHY §5.2.3].

## 11. CNR and MER performance

**Minimum CNR in AWGN** for post-FEC PER ≤ 1e-6 with 1500-byte packets, both directions, with each direction's level condition [normative, PHY §7.5.12.1 Table 46, §7.4.14.2 Table 18, checked on PDF p. 88, 148]:

| QAM | CM: CNR to 1002 MHz | CM: CNR 1002–1218 MHz | CM: min P6AVG (dBmV/6 MHz) | CMTS: CNR | CMTS: min set point (dBmV/6.4 MHz) |
|---|---|---|---|---|---|
| QPSK | n/a | n/a | n/a | 11.0 | −4 |
| 8 | n/a | n/a | n/a | 14.0 | −4 |
| 16 | 15.0 | 15.0 | −15 | 17.0 | −4 |
| 32 | n/a | n/a | n/a | 20.0 | −4 |
| 64 | 21.0 | 21.0 | −15 | 23.0 | −4 |
| 128 | 24.0 | 24.0 | −15 | 26.0 | 0 |
| 256 | 27.0 | 27.0 | −15 | 29.0 | 0 |
| 512 | 30.5 | 30.5 | −12 | 32.5 | 0 |
| 1024 | 34.0 | 34.0 | −12 | 35.5 | 0 |
| 2048 | 37.0 | 37.5 | −9 | 39.0 | 7 |
| 4096 | 41.0 | 41.5 | −6 | 43.0 | 10 |

Downstream:

- **It is CNR, not MER, and it is a requirement on the receiver.** The ratio is total signal over total noise in occupied bandwidth, for a 192 MHz occupied channel, with source noise and phase noise counted in the added AWGN.
- **Test conditions:** P6AVG ≤ 15 dBmV, spectrum up to 54–1218 MHz fully loaded, 4 adjacent 6 MHz channels ≤ P6AVG + 3 dB, any 6 MHz channel ≤ P6AVG + 6 dB, average per channel ≤ P6AVG + 3 dB, no reflections, burst noise or tilt.
- **The −15 dBmV floor holds only to 256-QAM.** 4096-QAM needs −6 dBmV/6 MHz or better.

**Relation to MER.** The downstream RxMER definition is `S_dB − E_dB`, data constellation power over pilot and preamble error power [normative, PHY §9.3.6]. In ideal AWGN, 35 dB data-subcarrier CNR gives about 35 dB RxMER, so the two scales coincide there. The same values serve as MER targets elsewhere:

- CM-OSSI's `CmDsOfdmRequiredQamMer` defaults in quarter-dB (60, 84, 96, 108, 122, 136, 148, 164, 184, 208) equal 15, 21, 24, 27, 30.5, 34, 37, 41, 46, 52 dB for 16 to 16384-QAM [normative, CM-OSSI §D.2.11 Table 72].
- TR-PMA uses Table 46 as its RxMER-to-QAM map [guideline, TR-PMA Appendix II].
- The SNR margin for a candidate profile is mean RxMER minus mean required RxMER, both averaged in dB across data subcarriers. The informative text ties required RxMER to CER = 1e-5 in AWGN, not 1e-6 PER. It also reports subcarriers at least x dB below threshold, default 1 [guideline, PHY Appendix VI, §9.3.6.1].

The operational thresholds in physics.md §4.6 (24, 26, 29, 32, 35 dB, for 256 to 4096-QAM) sit 3 to 6 dB below Table 46. That is a different quantity [inferred]: Table 46 is what any compliant CM must achieve under worst-case configurations, and the PNM-3.1 values are observed onset on the 2nd percentile. Don't mix them.

RxMER measurement conformance: standard deviation at most 0.5 dB, and 4 to 6 dB of change between 30 and 35 dB CNR, on a flat 192 MHz channel at nominal 6 dBmV/6 MHz [normative, PHY §9.3.6].

**Upstream conditions:** one 95 MHz channel, a pre-equalized single transmitter at the QAM's minimum set point, phase noise and frequency offset at CM limits [normative, PHY §7.4.14.2]. Boosted pilots add 1 dB at 50 kHz or 0.5 dB at 25 kHz [normative, PHY §7.4.14.2 Table 18 note 3]. The upstream requirement is 2 dB above downstream at each shared order, except 1.5 dB at 1024-QAM [inferred].

No PLC or NCP CNR requirement appears in the PHY (see Gaps).

## 12. Transmitter fidelity and noise

**CMTS downstream output MER** in a 192 MHz channel with 576 MHz occupied (any single subcarrier / channel average) [normative, PHY §7.5.9.1 Table 41]: below 600 MHz ≥ 48 / ≥ 50 dB; 600–1002 MHz ≥ 45 / ≥ 47 dB; 1002–1218 MHz ≥ 43 / ≥ 45 dB. 2 dB relief applies for minimal test receiver equalization. Inband noise and spurious in a 6 MHz gap: −50, −47, −45 dBr over the same ranges.

**CM upstream MER**, each minislot, flat channel, at `P1.6load = P1.6load_min_set` [normative, PHY §7.4.12.5.3.2 Tables 14–15]: with optimum pre-eq, ≥ 44 dB at 100% grant and ≥ 50 dB at 5% grant; no pre-eq, ≥ 40 dB. Up to 5 discrete spur subcarriers may be excluded. 4096-QAM upstream therefore has 1 dB between required CNR and the CM's own full-grant MER [inferred].

**CM upstream spurious** [normative, PHY §7.4.12.5.1 Table 11]: inband −45 dBc at 100% grant, −51 dBc at 5%, −50 dBc for S-CDMA and TDMA; between bursts −72 dBc. Also `SpurFloor = max(−57 + 10·log10(grant MHz/192), −60) dBc`. These hold only within 6 dB of `P1.6load_min_set`.

**Thermal floor:** the PHY states none. The −57.4 dBmV/6 MHz in physics.md §4.1 is ordinary physics, and the PHY neither confirms nor contradicts it.

## 13. Plant assumptions

These bound what FEC should handle at once. Stated as typical, and better plant is expected for higher capacity [guideline, PHY §5.2.2.1 Table 3, §5.2.2.2 Table 4, checked on PDF p. 36–37].

| Parameter | Downstream | Upstream |
|---|---|---|
| Transit delay | ≤ 0.4 ms | ≤ 0.4 ms |
| Composite noise | CNR ≥ 35 dB | C/(I+N) ≥ 25 dB, all noise, distortion, CPD, ingress, without impulse |
| CTB, CSO, XMOD, discrete ingress | ≥ 41 dB each | in the above |
| Amplitude variation per 6 MHz | ≤ 1.74 dB p-p | ≤ 2.78 dB p-p |
| Group delay variation | ≤ 113 ns over 24 MHz | ≤ 163 ns over 24 MHz |
| Group delay may exceed it | within about 30 MHz above the lower edge | within about 10 MHz of each band edge |
| Hum | ≤ −30 dBc (3%) | ≤ −26 dBc (5%) |

Dominant single echo bounds, as written:

| Echo delay | Downstream (dBc) | Upstream (dBc) |
|---|---|---|
| ≤ 0.5 µs | −20 | −16 |
| ≤ 1.0 µs | −25 | −22 |
| ≤ 1.5 µs | −30 | −29 |
| > 2.0 µs | −35 | −35 |
| > 3.0 µs | −40 | −42 |
| > 4.5 µs | −45 | −51 |
| > 5.0 µs | −50 | not listed |

The source leaves 1.5 to 2.0 µs undefined. Read it as a step function that gets stricter with delay, not an interpolated curve [inferred].

Network availability is assumed above 99.9% [guideline, PHY §5.2.2.2.1]. A 35 dB downstream assumption is below 4096-QAM's 41 dB, so the assumed plant doesn't carry top orders; profiles are meant to adapt [inferred, PHY §5.1].

**Pre-eq test case.** A −16 dB echo at 0.3125 µs causes 2.78 dB p-p variation, corresponding to about 16 dB MER. The CMTS must reduce variation by at least 0.8 dB, or 3 dB of MER gain, to about 19 dB, on a 22 MHz channel [normative, PHY §7.4.17]. This gives a checkable point for physics.md §5.2.

**OFDMA pre-eq format** [normative, PHY §7.4.17]: one complex coefficient per subcarrier, default 1+j0, zero on excluded subcarriers, resets to 1+j0 on leaving exclusion. Update is either replace or multiply, `C_k(i+1) = C_k(i) · A_k(i)`, in s1.14. Coefficients come from probes.

## 14. SC-QAM retained

Section 6 of the PHY is mostly references to D3.0 PHY and DRFI, so few numbers are in hand.

- **Upstream:** QPSK, 8, 16, 32, 64, 128-QAM; TDMA with optional S-CDMA; 1280, 2560, 5120 kHz rate on 1.6, 3.2, 6.4 MHz; bands 5–42, 5–65, 5–85 MHz [normative, PHY §6.2.24 Table 5]. Not above 85 MHz even with a higher split [normative, PHY §7.2.3, §7.2.4].
- **With a D3.1 CMTS,** the §7.4 transmit requirements apply even if all channels are SC-QAM, and reported power is per 1.6 MHz. There is no SC-QAM MER requirement in that mode [normative, PHY §6.2.19, §6.2.22, §7.4.12.2.1]. With a D3.0 CMTS, D3.0 rules apply [normative, PHY §6.2.19].
- **Downstream:** 6 MHz channels per CEA 542 and J.83-B [normative, PHY §1.1, §5.2.1.1]; detail is in documents we don't have (see Gaps). 23.5 dB Es/No at 64-QAM and 33 dB at 256-QAM appear, but as stated conditions on upstream timing tests, not as receiver thresholds [normative, PHY §7.4.2.1].
- **Coexistence:** an SC-QAM inside an OFDM channel is an exclusion band with 1 MHz edges [normative, PHY §7.2.5.1]. SC-QAM in or next to OFDM can degrade edge bit loading [field, TR-PMA §12.7].

## 15. Profile management in practice

**Roles.** The CMTS assigns and owns profiles. It may run its own logic or take recommendations from an external Profile Management Application, and it may decline them [guideline, TR-PMA §5.2.2, §12.2]. TR-PMA defines interfaces, not algorithms; the creation algorithm is vendor proprietary [guideline, TR-PMA §5.2.2, §7.2.3]. PMA doesn't manage the PLC, and NCP modulation is an operator choice [guideline, TR-PMA §7.1.1, §7.3].

**Lifecycle** [guideline, TR-PMA §6.1–§6.3, §7.2, §8.2]:

1. **Startup.** A new channel starts with one non-optimized profile or none. Profile A is the lowest common denominator, on every CM. Upstream the matching one is IUC 13 [TR-PMA Appendix III].
2. **Optimize.** Read OCD and DPD (UCD and IUCs upstream), the profile-to-CM list, RxMER per subcarrier via OPT or PNM files, FEC stats, then compute. Downstream test: `OPT` (RxMER, SNR margin, codeword and NCP stats). Upstream tests: probes for RxMER, `OUDP` for codeword counts.
3. **Change.** Free an unused profile by moving CMs, then redefine or add it, test with OPT, and assign if the CM passes. DOCSIS has no formal profile delete; the CMTS stops sending the DPD.
4. **Fallback.** On errors, the CMTS acts first, for speed, moving the CM to a less efficient profile such as Profile A, and may ask the PMA later. "Backup profile" is a PMA concept, not a DOCSIS one.

**Timing:** an OPT takes about 3 to 10 s; a full profile swap affecting many CMs takes tens of minutes or more [field, TR-PMA §6.2.3]. Profile changes can interrupt traffic briefly, so policy may block them during voice or video [guideline, TR-PMA §12.4]. One referenced method clusters CMs by MER signature and designs a profile per group [guideline, TR-PMA Appendix I.2].

**Testing a profile:** a transition profile tests a CM without carrying traffic, with its own counter set beside profiles 1–4 [normative, MULPI §7.12.1; PHY §9.3.7].

**Exclusions** are per channel, not per CM [normative, PHY §7.2.5.2]. OCD parameters can't change while the channel is in any CM's receive set [normative, MULPI §6.4.40].

## Implications for hfc-twin

All [inferred].

**Profile schema (`docs/tasks/01-schemas.md` §2b)**

1. Separate downstream and upstream channel types. The task spec says "OFDM blocks"; OFDMA also needs representing. Validate FFT and spacing per direction with §4's table; one implies the other, so storing both needs a check.
2. Add what the channel plan needs for codeword rates and echo tolerance: `cp_us`, `rolloff_us`, `time_interleaver_depth`, `f0_hz` or first and last active subcarrier, exclusions, PLC location, and upstream `minislot` count and pilot pattern.
3. PLC modulation shouldn't be a free field. It is always 16-QAM. NCP is an enum of QPSK, 16-QAM, 64-QAM. Data profile modulation for downstream should allow per-subcarrier ranges, since a single QAM per profile can't represent mixed modulation; 0 means zero-bit-loaded. Upstream profile data is per minislot.
4. `design_levels.modem_in_min_dbmv` should follow modulation (Table 46's Min P6AVG for the profile's highest order) rather than a flat −15.
5. `design_levels` should also carry the 10 dB PSD spread limit (§9) as a check, alongside `tilt_tolerance_db`.
6. `tx_max_psd_dbmv_per_1p6mhz` is not a modem constant. Store `Pmax` per modem (`tx_max_total_dbmv`) and compute `P1.6hi` from the TCS, capped at 53.2 + (Pmax − 65) for 24 MHz or less.
7. `us_node_in_target_dbmv` should be checked against the CMTS set point table, once a return path exists.

**physics.md**

1. §3.3: add the Table 45 relative PSD check and Table 46's level floor per QAM. The whole-fixture oracle's 13.87 dB spread at one modem exceeds a 10 dB limit if that modem demodulates both band ends.
2. §4.1: mapping and noise figures are consistent with the PHY; keep them.
3. §4.5: `MER_impl` isn't given by the PHY for the CM. Table 46's requirement at 41 to 41.5 dB CNR, and CMTS output MER of 43 to 48 dB, are the only anchors. Add a frequency-dependent CMTS output term if ceilings look too flat at high frequency. RxMER data-subcarrier power is 0.2 to 0.3 dB below the 6 MHz level scaled by subcarrier count.
4. §4.6: label which thresholds are which. Keep the guide's onset values for failure physics. Use Table 46 and `CmDsOfdmRequiredQamMer` defaults for any emitted SNR margin, which is what a real CM reports. physics.md's starred 12, 18, 21 dB are its own extrapolation; the spec's CNR values for those orders are 15, 21, 24.
5. §4.6: codewords/s, if one profile fills the channel, is `Σ_k bits_k / (16200 · (T_U + T_CP))` over data subcarriers. Example: 8K, all 4096-QAM, 2.5 µs CP, 190 MHz, about 7,469 data subcarriers, NCP ignored: about 130,000 codewords/s, 2.11 Gbit/s raw, 1.85 Gbit/s after FEC.
6. §5: echoes beyond CP add `(τ − T_CP)·A²/T_U` of ISI and about as much ICI, on top of the frequency ripple. Add as a noise term in §4.5, tied to the profile's CP. CP in the channel plan then determines whether a far echo hurts.
7. §7.2: replace the "53.2" limit test with `P1.6r ≤ P1.6hi(Neq)` (48.0 on an 80 MHz TCS, 44.2 on 192 MHz). Keep 17 as the floor, with boosted-pilot offsets. Partial service is one possible result; lower profile and MSM are the others the PHY names.
8. §7.2: `0 dBmV/6.4 MHz` supports up to 1024-QAM only. Raising to 2048 or 4096 needs 7 or 10.
9. §7.2/§7.3: 15 dBmV/6.4 MHz fully loaded on 80 MHz is 26 dBmV total. If that is the level at the laser, it exceeds the digital laser's 25 dBmV linear limit and the worked 22.6 and 20.6 dBmV budget maxima, though not the analog 27 dBmV limit. Decide what `P_in,node` refers to before modelling laser clipping.
10. §8: faults could include return laser overdrive, using TR-MIDSPLIT's clipping formula rather than a lookup.

**Sim behaviour**

- Each OFDM channel has one set of exclusions, and many profiles. Exclusions must stay common to profiles.
- FEC stats per profile plus transition, PLC and NCP as physics.md §9 says. PLC is 16-QAM at rate 3/4 and NCP is rate 1/2, both stronger than data at the same order at rate 8/9. No spec value exists for where they fail (see Gaps), so any threshold is an estimate.
- A profile manager in the sim should emulate step 4 in §15: fast CMTS fallback to a lower profile on error, and slow reoptimization in tens of minutes. Changes in telemetry following a profile change are expected behaviour, not faults.

## Gaps

- **D3.0 PHY and DRFI aren't in the corpus.** SC-QAM downstream symbol rates, thresholds and J.83-B detail, and D3.0 fidelity are by reference only. physics.md's 26 dB SC-QAM threshold can't be checked here.
- **PLC and NCP CNR.** The PHY has no minimum CNR table for them. Failure points in the sim need to be estimated from their FEC and modulation.
- **Above 1218 MHz.** Table 46 stops at 1218 MHz, with no CNR for 1794 MHz operation or for 8192 and 16384-QAM; only the OSSI and TR-PMA list values for those orders (46 and 52 dB).
- **CM implementation ceiling.** No CM receive MER limit in the PHY.
- **Tilt.** The word doesn't appear in Table 45. The 10 dB spread is our reading, and we haven't found a statement that X applies across a whole receive channel set versus one channel. Check against plant-and-powering notes and the PNM guide before making it a hard validity rule.
- **N_I bound.** The stated maxima 7537 and 3745 imply 47 continuous pilots at full width, but the §7.5.15.2 formula gives at least 56 at 190 MHz (M ≥ 48 plus 8). We couldn't reconcile this; it has little effect on the sim.
- **192 vs 3841.** §7.5.7.1 says 192 MHz is 3841 or 7681 subcarriers; §5.1 says 3840 or 7680.
- **High-split channel arithmetic.** Two 95 MHz encompassed channels give 190 MHz, while §7.2.3 says 192 MHz of active channels at 204 MHz. Our reading is occupied bandwidth; the PHY doesn't say.
- **Neq formula.** §7.4.12 uses a ceiling on both terms; §7.4.12.3.1 applies it only to OFDMA.
- **Echo table.** No value for 1.5 to 2.0 µs, and the upstream has no bound above 4.5 µs other than −51 dBc.
- **Diplexers and amps in migration.** TR-MIDSPLIT doesn't address them; plant effects in §3 are ours.
- **Not recorded:** upstream interleaving, and the pilot layouts in Figures 39–44. Not needed yet.
- **TR-MIDSPLIT age.** 2015, from two operators. Its laser numbers are one population, not a general rule.
