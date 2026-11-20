# Validation against published results

`docs/physics.md` §12, run by `crates/sim/tests/validation.rs`. No real measurements are available, so published results are the only
outside check. This shows the sim reproduces what CableLabs measured in those setups. It doesn't show the sim matches any plant.

| Target | Published | Sim | Tolerance | Result |
|---|---|---|---|---|
| OFDM uncorrectable, 50 %, 256-QAM | 23.45 dB | 23.45 dB | ±1 dB | Pass |
| OFDM uncorrectable, 50 %, 4096-QAM | 34.45 dB | 34.45 dB | ±1 dB | Pass |
| OFDM correctable, 50 %, 256-QAM | 6.8 dB margin | 6.8 dB | ±1 dB | Pass |
| Narrowband ingress, 4096-QAM, 192 MHz, at onset (1e−3) | near 8–9 dB | about 9 dB | ±2 dB | Pass |
| Narrowband ingress, 4096-QAM, 96 MHz, at half | near 15 dB | about 15 dB | ±2 dB | Pass, but the two rows read different points |
| −10 dBc at 0.5 µs, tap 10 | −17.1 dB | −15.5 dB | ±2 dB | Pass |
| −10 dBc at 0.5 µs, tap 11 | −14.4 dB | −12.8 dB | ±2 dB | Pass |
| −10 dBc at 0.5 µs, NMTER | −11.61 dB | −10.5 dB | ±1 dB | **Miss by about 0.1 dB** |
| Band-edge group delay, N+5, NMTER | −15.9 dB | about −15.9 dB, by setting the table | ±3 dB | Pass, but fitted |
| Echo ripple | 5.69 dB, 2 MHz, 53.5 ft | exact | ±0.1 dB, ±0.5 ft | Pass |
| RxMER on healthy plant | 41.2 dB | 41–42 dB | ±2 dB | Pass |
| Analog link CNR, SCTE 270 §25.7.6, 4 MHz, with EDFA | 52.99 dB | 52.98 dB | ±0.02 dB | Pass (task 22) |
| Same, without the EDFA | 55.33 dB (our arithmetic on the source's values) | 55.33 dB | ±0.02 dB | Pass (task 22) |
| Return laser at 7 dBmV total, TR-MIDSPLIT §6.1.1.1 | 35.5 dB | 35.52 dB | ±0.5 dB | Pass, but fitted (task 22) |
| Return laser at 26 dBmV total | 52.5 dB | 52.54 dB | ±0.5 dB | Pass, but fitted (task 22) |
| 10GBASE-LR margin at 1.32 dB of loss, launch mid-range | 9.23 dB (our arithmetic on IEEE 802.3's limits) | 9.2294 dB | 0.001 dB | Pass (task 22) |
| Fiber delay, TR-DCA §11.9.3 | 500 µs per 100 km | 1.524 µs per 1,000 ft | exact | Pass (task 22) |

**The miss.** The sim inverts the channel, which carries about 1.1 dB more off-main energy than the published string. Folding through the matched
filter closed this from 1.5 dB and moved taps 9 and 12 to within 0.4 dB, and that's physics we'd want anyway. Truncating to 24 taps changed
nothing. Candidates for the rest: the field equalizer may stop adapting short of a full inverse, or the lab's echo may not be exactly −10 dBc. Neither
has a source, so nothing was added to close it, and the tolerance wasn't widened. `reflection_nmter_known_miss` fails if the gap grows or closes.

**The narrowband rows.** The guide's figures don't say what rate "uncorrectables" means, and §4.6's cap was fitted to these same tests. Read at one
rate, 1e−3, the 96 MHz case comes to about 23 dB, not 15 dB. So these rows check the fit, not an independent result.

**The fitted row.** §12 said to set the `group_delay_ns` tables against this result, so passing it says nothing independent. The edge value went from
50 to 36 ns per active, or 216 ns through N+5, which fits the guide's "up to about 300 ns".

**Task 22's rows.** The return laser rows are fitted: `rtx-generic-1310-npr`'s modulation index and RIN were chosen to land on the two published
points, under an 80 MHz load and −6 dBm received that the source doesn't state (approx), so passing them checks the model's shape, not an
independent result: it peaks near 25 dBmV and has fallen 12 dB by 28, where the source's usable range ends at 27. The SCTE 270 rows check the four
terms against the source's own arithmetic, and the digital and delay rows are unit conversions of the standards' limits.
