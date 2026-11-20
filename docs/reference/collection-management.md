# Collection and management of PNM and status data

How real CMs, CCAPs and plant actives expose PNM and status data, and what an adapter must do to read it.

`docsX` names are SNMP objects and `CamelCase` names are OSSI model attributes. `...` abbreviates the table prefix.

## 1. Who measures what

CM and CCAP test points act as spectrum, vector signal and network analyzers on the plant. Both must measure in
service unless stated otherwise, with timestamps accurate to 100 ms or better [PHY §9.2] [normative].

**CM tests**, all per CM [CM-OSSI §D.2.4 to §D.2.14] [normative]. Default file name is `<name>_<CM MAC>_<epoch>`.

| Test | Table | Key | Start with | File type, name |
|---|---|---|---|---|
| DS RxMER | `docsPnmCmDsOfdmRxMerTable` | OFDM ifIndex | `...RxMerFileEnable` | 504E4E04 `PNMDsMer` |
| DS channel estimate | `docsPnmCmOfdmChanEstCoefTable` | OFDM ifIndex | `docsPnmCmOfdmChEstCoefTrigEnable` | 504E4E02 `PNMChEstCoef` |
| Constellation | `docsPnmCmDsConstDispMeasTable` | OFDM ifIndex | `...ConstDispTrigEnable` | 504E4E03 `PNMDSConDisp` |
| Symbol capture | `docsPnmCmDsOfdmSymCapTable` | OFDM ifIndex | `docsPnmCmDsOfdmSymTrigEnable` | 504E4E01 `PNMSymCap` |
| FEC summary | `docsPnmCmDsOfdmFecTable` | OFDM ifIndex | `...FecFileEnable` | 504E4E08 `PNMFecSum` |
| Modulation profile | `docsPnmCmDsOfdmModProfTable` | OFDM ifIndex | `...ModProfFileEnable` | 504E4E0A `PNMModProfile` |
| DS histogram | `docsPnmCmDsHistTable` | RF MAC ifIndex | `...HistEnable` | 504E4E05 `PNMHist` |
| US OFDMA pre-eq | `docsPnmCmUsPreEqTable` | OFDMA ifIndex | `...PreEqFileEnable` | 504E4E06 `PNMUsPreEq`, 504E4E07 `PNMUsPreEqLastUpdate` |
| MER margin | `docsPnmCmDsOfdmMerMarTable` | OFDM ifIndex | `...MerMarMeasEnable` | none, read objects |
| Full-band capture | `docsIf3CmSpectrumAnalysisCtrlCmd` (scalars) | none | `...Enable`, then `...FileEnable` | 504E4E09 `PNMSpecAnData` |

Full-band capture lives in DOCS-IF3-MIB, not DOCS-PNM-MIB [PNM-HFC §5.2.8].

**CCAP tests** [CCAP-OSSI §7.3.4 to §7.3.6, §A.4] [normative]. Default name is
`<name>_<CCAP id>_<YYYY-MM-DD_HH.MM.SS.mmm>_<ifIndex>`.

| Test | Table | Scope | Start with | File type, name |
|---|---|---|---|---|
| DS symbol capture | `docsPnmCmtsDsOfdmSymCapTable` | channel | `docsPnmCmtsDsOfdmSymTrigEnable` | 504E4E65 `PNMCcapSymCap` |
| DS NPR notch | `docsPnmCmtsDsOfdmNoisePwrRatioTable` | channel, out of service | `docsPnmCmtsDsOfdmNprEnable` | none; the CM measures |
| US active or quiet probe | `docsPnmCmtsUsOfdmaAQProbeTable` | channel; one CM or idle SID | `...AQProbeEnable` | 504E4E66 `PNMCcapAQProbe` |
| US impulse noise | `docsPnmCmtsUsImpNoiseTable` | US RF port | `...ImpNoiseEnable` | 504E4E67 `PNMCcapImpNoise` |
| US histogram | `docsPnmCmtsUsHistTable` | US RF port | `...HistEnable` | 504E4E68 `PNMCcapHist` |
| US OFDMA RxMER | `docsPnmCmtsUsOfdmaRxMerTable` | channel; one CM (`...RxMerCmMac`) | `...RxMerEnable` | 504E4E69 `PNMCcapRxMER` |
| US OFDMA Rx power | `docsPnmCmtsUsOfdmaRxPwrTable` | channel; one CM | `...RxPwrEnable` | none, read `...RxPwrOnePtSixPsd` |
| UTSC | `docsPnmCmtsUtscCfgTable`, `...CtrlTable`, `...StatusTable`, `...ResultTable` | ifIndex plus row index | `docsPnmCmtsUtscCtrlInitiateTest` | 504E4E6A `PNMCcapUsSpecAn` |
| OFDM profile test | `docsPnmCmtsOptReqTable`, `docsPnmCmtsOptRespTable` | one CM, channel, profile | row create, `...OptReqProfileTest` bits | none |

OPT is the only standard route by which a CMTS returns a CM's per-subcarrier DS MER: `docsPnmCmtsOptRespMerData`, up to
7680 octets in the one-byte RxMER encoding, from the first active subcarrier. It also returns codeword and NCP counts.
It targets a profile the CM is not currently using, and a full MER response exceeds one SNMP PDU in an Ethernet frame
[CCAP-OSSI §7.3.6, §7.3.6.3] [normative].

## 2. How a test runs

Sequence for CM and CCAP [PNM-HFC §6.2.6.5, §8.4.2] [guideline]: SNMP SET the bulk destination; SNMP SET the test; poll
status; the device writes the file to the TFTP server; read any SNMP results; the application takes the file from the
TFTP server.

**CM test lock** [CM-OSSI §D.2.3] [normative]. `docsPnmCmCtlTest` names the current or last test;
`docsPnmCmCtlTestDuration` counts seconds in progress (informative); `docsPnmCmCtlStatus` is other 1, ready 2,
testInProgress 3, tempReject 4. A covered test starts only at ready, so one instance of one covered test runs at a
time. Otherwise the SET returns `inconsistentValue` or the status shows resourceUnavailable. Covered: spectrum, symbol
capture, channel estimate, constellation, RxMER, FEC, histogram, US pre-eq. MER margin and modulation profile are not in
the enumeration [inferred]. MER margin has its own lock, one active row [CM-OSSI §D.2.9].

**MeasStatus**, on every test: other 1, inactive 2, busy 3, sampleReady 4, error 5, resourceUnavailable 6,
sampleTruncated 7, interfaceModification 8 [CM-OSSI §D.2.1, CCAP-OSSI §7.3.2.2]. sampleTruncated means valid data with
fewer bins than asked [CM-OSSI §D.2.4]. interfaceModification means a DBC or primary-backup change renumbered
interfaces [CM-OSSI §D.2.1], so re-resolve ifIndex keys [inferred].

**Aborts and refusals** [normative]. The CM aborts all tests on a DBC-REQ that changes its RCS or on a switch to the
backup primary [CM-OSSI §D.1.1]. Most triggers return `inconsistentValue` in DLS or battery-backup mode [CM-OSSI §D.2.5
to §D.2.14]. The CM may disable the spectrum analyzer to protect service [CM-OSSI §D.2.4]. Load balancing aborts a CM
test, and modems can be placed in the load-balance exception table. A channel reset or admin-down ends tests on it
[CCAP-OSSI §7.3.1].

**No test needed.** RxMER summaries are measured when read, with `Percentile` default 2 [CM-OSSI §D.2.8]. Channel-estimate
and pre-eq summaries compute on query [CM-OSSI §D.2.6, §D.2.13]. FEC samples accumulate in the background from when the CM
is operational, and the test only returns them [CM-OSSI §D.2.10].

**Reaching the CM.** SNMP goes to the CM's own IP on UDP 161 (v2c in examples): first ask the CMTS for MAC to IP, then
poll the CM [PNMP App. II] [guideline]. The D3.1 mapping is `docsIf3CmtsCmRegStatusMacAddr` with `...IPv4Addr`,
`...IPv6Addr` [CCAP-OSSI §A.1]. No listed source describes a CMTS SNMP proxy for CM MIBs [inferred, from searching them].

**Details.** Spectrum needs `Enable` true before `FileEnable`; configuration writes may need a re-enable, and
`InactivityTimeout` (default 300 s, 0 never) auto-disables [CM-OSSI §D.2.4.1]. Symbol capture waits for a PLC Trigger
Message Block with its `TrigGroupId`, timing out after `...TrigEnableTimeout` (default 600 s, up to 3600); run the CCAP
side with the same group [CM-OSSI §D.2.5]. File names can change only when not busy; an empty name makes a default.

## 3. MIB names and identifiers

Use exact names: compliance tables abbreviate model attributes, e.g. `ThresholdRxMerValue` is
`docsPnmCmDsOfdmRxMerThrVal` [CM-OSSI §A.1, §D.2.8].

OIDs seen in sources. Subtree labels are inferred from the objects under them. CM test tables, bulk transfer tables and
UTSC have no OID in any source (see Gaps).

- DOCS-IF `1.3.6.1.2.1.10.127`; DOCS-IF3 `1.3.6.1.4.1.4491.2.1.20`; DOCS-PNM `...4491.2.1.27`; DOCS-IF31 `...4491.2.1.28`.
- Under `...4491.2.1.27` [PNM-HFC §6.3.1] [field, one vendor's method per the source]:
  `.1.1.1.0` `docsPnmBulkCtl`; `.1.1.1.{1,2,3}.0` `docsPnmBulkDest` `IpAddrType`, `IpAddr`, `Path`;
  `.1.1.2.1.{2,4}` `docsPnmBulkFile` `Name`, `UploadStatus` (printed with `.0`, though table columns [inferred]);
  `.1.2.1.{1,2,3}.0` `docsPnmCmCtl` `Test`, `TestDuration`, `Status`;
  `.1.3.7.1.{1..6}.<ofdma ifIndex>` `docsPnmCmtsUsOfdmaRxMer` `Enable`, `CmMac`, `PreEq`, `NumAvgs`, `MeasStatus`, `FileName`.
- `...4491.2.1.20.1.3.1.2` `docsIf3CmtsCmRegStatusMacAddr` [PNM-HFC §6.3.1].
- `...4491.2.1.28.1.3.1.3` `docsIf31CmtsCmRegStatusUsProfileIucList` finds a CM's OFDMA ifIndex [PNM-HFC §6.3.1]: repeated
  4-octet ifIndex, 1-octet IUC count (1 to 2), IUC octets (5, 6, 9 to 13). `0x00000024020506` is ifIndex 36, IUCs 5, 6.
- `...4491.2.1.20.1.34.{1..9}` `docsIf3CmSpectrumAnalysisCtrlCmd`: Enable, InactivityTimeout, First, Last, Span, NumBins,
  ENB, Window, NumberOfAverages. `...20.1.35.1.{1,2,3}` Meas table: Frequency, AmplitudeData, TotalSegmentPower [PNMP §IX.2].
- Under `1.3.6.1.2.1.10.127`, example ifIndex stripped [inferred] [PNMP App. II, III]: `.1.1.1.1.{2,6}` `docsIfDownChannel`
  `Frequency`, `Power`; `.1.1.4.1.{2,3,4}` `docsIfSigQ` `Unerroreds`, `Correcteds`, `Uncorrectables`; `.1.3.7.1.2`
  `docsIfCmtsCmPtr`; `.1.3.3.1.{3,8}` `docsIfCmtsCmStatus` `IpAddress`, `EqualizationData` (deprecated table);
  `.1.2.2.1.{12,13,17}` `docsIfCmStatus` `T3Timeouts`, `T4Timeouts`, `EqualizationData` (deprecated).

## 4. Bulk data and TFTP

**The device that ran the test uploads it.** A CM uploads CM files and a CCAP uploads CCAP files. Each is a TFTP client (RFC
1350 over UDP) that opens on port 69 in `octet` mode and does a write, with the RFC 2348 blocksize option: 1448 on IPv4, 1428
on IPv6 [CM-OSSI §D.5.1, CCAP-OSSI §7.5.1.1] [normative]. Neither is required to retry automatically [CM-OSSI §D.5.1].

**CM control** [CM-OSSI §D.5.3] [normative]. `docsPnmBulkDestIpAddrType` (unknown 0, ipv4 1, ipv6 2), `docsPnmBulkDestIpAddr`,
`docsPnmBulkDestPath` (excludes file name, includes delimiters, e.g. `/pnm/`), writable only when `docsPnmCmCtlStatus` is
ready. `docsPnmBulkUploadControl`: noAutoUpload 2, autoUpload 3 (default). `docsPnmBulkFileTable`: `...FileIndex`, `...FileName`,
`...FileControl` (tftpUpload 2, cancelUpload 3, deleteFile 4), `...FileUploadStatus` (availableForUpload 2, uploadInProgress 3,
uploadCompleted 4, uploadPending 5, uploadCancelled 6, error 7). One upload at a time across the table.

**CM storage** [CM-OSSI §D.5.2] [normative]. At least 64 KB per file and 4 files. When full, the oldest file is overwritten
regardless of type. If that one is uploading, `docsPnmCmCtlStatus` becomes tempReject 4.

**CCAP control** [CCAP-OSSI §7.5.1.3 to §7.5.1.7, §A.4] [normative]. `docsPnmCcapBulkDataControl*` and `docsPnmBulkFile*` are
deprecated on CCAP. Current: `docsPnmBulkDataTransferCfgTable` (`DestIndex`, `DestHostname`, `DestHostIpAddrType`,
`DestHostIpAddress`, `DestPort`, `DestBaseUri`, `Protocol` tftp 1, http 2, https 3 not supported in 3.1, `LocalStore` default
true, `RowStatus`); `docsPnmBulkFileStatusTable`; `docsPnmBulkFileMgmtTable`. Each test's `...DestinationIndex` picks the row; zero
means keep locally. Results should be kept an hour or more; overwrite and concurrency are vendor policy [CCAP-OSSI §7.3.1,
§7.5.1.2]. TFTP for all tests is a MUST but not universally supported [PNM-HFC §8.4.6] [field].

**Network** [PNM-HFC §8.4.3] [guideline]. TFTP uses ephemeral ports, so firewalls must pass UDP 69 and the ephemeral range to the
server, including from CM subnets. TFTP and SNMP are unencrypted. Lost UDP means lost data; the application detects that and
reruns the test.

**DAA and newer.** UTSC from an RPD goes over an L2TP pseudowire (IP protocol 115) in PSP format, with vendor-specific setup,
seen at 80+ Mbps per mid-split port [PNM-HFC §6.4.4] [field]. gNMI (CCAP-OSSI v4.0) and Kafka are emerging [PNM-HFC §8.4.4].

## 5. PNM file formats

**Common header** [CM-OSSI §D.2.2] [normative]. File type (4), major (1), minor (1), capture time (4, Unix seconds),
right-justified and zero-padded. Prefix `504E4D` has no version bytes; prefix `504E4E` has them. 504E4D09 and 504E4E09 both
exist for spectrum [PNM-HFC §5.2.8.10, SCTE 283 §7.4.2.2.2.1]. CM files have major 1. Maximum size is vendor-defined. Fields are
big-endian [inferred from the FEC hex example, CM-OSSI §D.2.10].

CCAP files add ifIndex (4), unique CCAP id (256, null-terminated), RPD id (6), RPD port (1). Major is 2, or 3 for AQ probe
[CCAP-OSSI §7.3.3.1, §7.3.5.1, §7.3.5.5.7].

**CM layouts.** Fields and sizes [CM-OSSI §D.2.4 to §D.2.14] [normative]; byte offsets computed [inferred]. All start 0 type, 4
major, 5 minor, 6 capture time, except FEC.

| File | Then at offset | Data at |
|---|---|---|
| RxMER 04, channel estimate 02 | 10 DS chan id; 11 CM MAC; 17 subcarrier-zero freq (Hz); 21 FirstActiveSubcarrierIndex (2); 23 spacing kHz (1); 24 length (4) | 28 |
| Constellation 03 | 10 chan; 11 MAC; 17 SC0; 21 modulation order (2); 23 samples (2); 25 spacing; 26 length | 30 |
| Symbol capture 01 | 10 chan; 11 MAC; 17 SC0; 21 sample rate (4); 25 FFT size (4); 29 trigger group (2); 31 transaction (1); 32 length | 36 |
| US pre-eq 06, last update 07 | 10 US chan; 11 CM MAC; 17 CMTS MAC; 23 SC0; 27 first active (2); 29 spacing; 30 length | 34 |
| Spectrum 09 | 10 chan (always 0); 11 MAC; 17 first center; 21 last center; 25 span (Hz, 4 each); 29 bins (2); 31 ENB (2); 33 window (2); 35 length | 39 |
| Histogram 05 | 10 CM MAC; 16 symmetry (1); 17 dwell length (4); dwell values; hit length (4); hit values | 21 |
| Modulation profile 0A | 10 chan; 11 MAC; 17 profile count (1); 18 SC0; 22 first active; 24 spacing; 25 length | 29 |
| FEC 08, no capture time | 6 chan; 7 MAC; 13 summary type (1); 14 profile count (1) | 15 |

CCAP US RxMER 504E4E69 [CCAP-OSSI §7.3.5.5.7], offsets [inferred]: 10 ifIndex; 14 CCAP id; 270 RPD id; 276 RPD port; 277 CM MAC; 283
averages (2); 285 pre-eq on/off (1); 286 SC0; 290 first active (2); 292 spacing (1); 293 length (4); data at 297.

**RxMER** [CM-OSSI §D.2.1.3, CCAP-OSSI §7.3.2.5] [normative]. One byte per subcarrier: `dB = value / 4` (23.75 dB is 0x5F). Range 0 to
63.5 dB, clipped at 0x00 and 0xFE. 0xFF means no measurement. Runs low to high including excluded subcarriers, no gaps. Entry k is at
`SC0 + (FirstActive + k) * spacing` [inferred]. Sources word FirstActive two ways: lowest in the Encompassed Spectrum (header), lowest
active (data type).

**Complex** [CM-OSSI §D.2.1.2, CCAP-OSSI §7.3.2.1] [normative]. 16-bit two's complement `sm.n`, I in the high 16 bits, Q in the low 16,
clipped independently. Value = raw / 2^n. `0x2400F800` in s2.13 is 1.125 − 0.25j.

| Data | Format | 1.0 is | Notes |
|---|---|---|---|
| Channel estimate 02 | s2.13 | 0x2000 | Average power about 1. Excluded: 0x8000 on I and Q. An active one computing to that reads 0x8001 [CM-OSSI §D.2.6] |
| Pre-eq current 06 | s2.13 | 0x2000 | Power averaged over coefficients normalized to 1 [CM-OSSI §D.2.13] |
| Pre-eq last update 07 | s1.14 | 0x4000 | RNG-RSP values [CM-OSSI §D.2.13] |
| Constellation 03 | s2.13 | 0x2000 | Up to 8192 samples per query, data subcarriers [CM-OSSI §D.2.7] |
| Symbol capture 01, 65 | s3.12 | 0x1000 | CM time domain; CCAP frequency domain before IFFT [CM-OSSI §D.2.5, CCAP-OSSI §7.3.4.1] |

**SC-QAM equalizer** (`DocsEqualizerData`) [CM-OSSI §7.1.3.7] [normative]. Main tap location, taps per symbol, forward taps n, reverse taps m
(1 byte each), then signed 16-bit real and imaginary per tap; up to 64 taps, 260 bytes. `08 01 18 00` is main tap 8, 1 per symbol, 24 forward
[PNM-HFC §6.2.6.4.1]. Scale varies: common D2.0 modems have maximum amplitude 2047 or 511, some with 3-nibble two's complement [PNM-HFC
§6.2.6.2] [field]. MTC, MTR and NMTER are not MIB objects; the application computes them [PNM-HFC §6.2.6.4.2].

**Spectrum.** Bins are 16-bit two's complement, hundredths of a dB, referenced to 0 dBmV [CM-OSSI §D.2.1.1, §D.2.1.4]. Over SNMP,
`docsIf3CmSpectrumAnalysisMeasAmplitudeData` per segment has center, span, bins, spacing, RBW (4 bytes each), then amplitudes;
`...MeasTotalSegmentPower` is tenth dBmV summed in power [CM-OSSI §D.2.4.2]. Defaults 93 to 993 MHz, 7.5 MHz span, 256 bins, ENB 150
(hundredths of bin spacing), 1 average. Limits 2 to 2048 bins, ENB 50 to 500, 1 to 1000 averages (leaky integrator, alpha = 1 − 1/N). Window: other 0,
hann 1, blackmanHarris 2, rectangular 3, hamming 4, flatTop 5, gaussian 6, chebyshev 7. Segments = (Last − First) / Span + 1 [CM-OSSI §D.2.4.1]. Readings may
include AGC and filtering [CM-OSSI §D.2.4]. Calibrate by comparing `TotalSegmentPower` with `docsIfDownChannelPower` over the same 6 MHz channels, averaging
in linear power; one example read 3.04 versus −0.1 dBmV [PNM-HFC §5.2.5] [field].

**FEC data** [CM-OSSI §D.2.1.6, §D.2.10] [normative]. Per profile: id (1), set count (2), then sets of timestamp, total, corrected, uncorrectable (4 bytes each),
oldest first. 600 sets at 1 per second, or 1440 at 1 per minute; fewer if not up that long. Cleared on reset; kept across Re-Init MAC with a time gap. **The spec's hex
example shows a 1-byte count (`ff 05` then the timestamp), but the definition says 2 and 1440 needs 2. Follow the definition; check real files [inferred].**

**Histogram** [CM-OSSI §D.2.12]. Even symmetry ('false', default) has 256 bins with centers at value + 0.5. Odd ('true') has 255, bin 0 unused. Dwell may be a single value.
"Length" is headed bytes but described as a count [inferred: check real files].

**Modulation profile** [CM-OSSI §D.2.1.7, §D.2.14]. Id (1), length (2), runs of type 0 (assignment 1, count 2) or type 1 skip (main 1, skip 1, count 2). Assignments: 0 zero-bit, 1
continuous pilot, 2 QPSK, 4 16-QAM, 6 64-QAM, 7 to 14 are 128- to 16384-QAM, 16 exclusion, 20 PLC.

**UTSC** [CCAP-OSSI §7.3.5.6.2, §7.3.5.6.3.18] [normative]. Adds LogicalChIfIndex, CenterFreq, FreqSpan, NumBins, Averaging, SamplingRate, OutputFormat, FileLen, NumSampleSets,
FftSampleSetSize, CalibrationConstantK (hundredth dB), Window, then repeated capture time and SampleData. timeIQ and fftPower (4 bytes) are mandatory; rawAdc, fftIQ, fftAmplitude
and fftDb (hundredth dBmV, 2 bytes) are optional.

**Required MER defaults**, `docsPnmCmDsOfdmReqMerQam*`, quarter-dB and writable, shown in dB [CM-OSSI §D.2.11]: 16-QAM 15, 64 21, 128 24, 256 27, 512 30.5, 1024 34, 2048 37, 4096 41,
8192 46, 16384 52.

## 6. Status tables

**CM summaries** [CM-OSSI §A.1, §D.2.6, §D.2.8, §D.2.9, §D.2.13] [normative].

| Object | Unit |
|---|---|
| `docsPnmCmDsOfdmRxMerMean`, `...StdDev` | hundredth dB, population, on dB values over active subcarriers |
| `docsPnmCmDsOfdmRxMerThrVal` | quarter dB, sorted value at floor(N·Percentile/100), 1-based (73 of 3677 at 2%) |
| `docsPnmCmDsOfdmRxMerThrHighestFreq` | Hz, highest-frequency subcarrier at that value |
| `docsPnmCmOfdmChEstCoef` `AmpRipplePkToPk`, `AmpRippleRms`, `AmpMean`, `AmpSlope` | 0.001 dB; slope 0.001 dB/MHz |
| `...GrpDelayRipplePkToPk`, `...GrpDelayRippleRms`, `...GrpDelayMean`, `...GrpDelaySlope` | 0.001 ns; slope 0.001 ns/MHz |
| `docsPnmCmUsPreEq` with the same suffixes, plus `...PreEqCoAdjStatus` | other 1, success 2, clipped 3, rejected 4 |
| `docsPnmCmDsOfdmMerMar` `MeasuredAvgMer`, `AvgMerMargin`, `ReqAvgMer`, `ThrshldOffset` | first two hundredth dB, last two quarter dB |

Fit and group-delay definitions are in [CM-OSSI §D.4]. Summaries are not stored in pre-eq files [CM-OSSI §D.2.13].

**CM status and channel** [CM-OSSI §F.2.3, §F.3, §A.1] [normative].
- `docsIf3CmStatusValue` (CmRegState), `...Code`, `...Resets`, `...LostSyncs`, T1, T2. CmRegState: notSynchronized 3, phySynchronized 4, usParametersAcquired 5, rangingComplete 6, dhcpV4Complete 7,
  registrationComplete 11, operational 12, accessDenied 13, forwardingDisabled 20, rfMuteAll 23.
- SC-QAM US, `docsIf3CmStatusUsTable`: `TxPower` (tenth dBmV, total channel power, the same on a 3.0 or 3.1 CMTS [CM-OSSI §F.2.3.2.2]), `T3Timeouts`, `T4Timeouts`, `T3Exceededs`, `RangingAborteds`,
  `EqData`, `IsMuted`, `RangingStatus` (aborted 2, retriesExceeded 3, success 4, continue 5, timeoutT4 6). `docsIf31CmUsScQamChanTxPsd`: quarter dBmV per 1.6 MHz [CM-OSSI §F.3.3.3.1.20].
- OFDMA US: `docsIf31CmStatusOfdmaUs` `T3Timeouts`, `T4Timeouts`, `T3Exceededs`, `RangingAborteds`, `IsMuted`, `RangingStatus`. `docsIf31CmUsOfdmaChanTxPower`: quarter dBmV, target power P1.6r_n [CM-OSSI §F.3.3.3.3.11].
- SC-QAM DS: `docsIfDownChannelPower` (tenth dBmV, last value or 0 when down [CM-OSSI §F.3.2.3.1.7]); `docsIfSigQ*` and `docsIfSigQExt*` counters, no OFDM rows [CM-OSSI §7.1.3.7]; `docsIf3SignalQualityExtRxMER`.
- OFDM channel: `docsIf31CmDsOfdmChan` `PlcTotalCodewords`, `PlcUnreliableCodewords`, `NcpTotalFields`, `NcpFieldCrcFailures` (Counter64), `SubcarrierZeroFreq`, `FirstActiveSubcarrierNum`,
  `LastActiveSubcarrierNum`, `SubcarrierSpacing`, `PlcFreq`.
- OFDM per profile: `docsIf31CmDsOfdmProfileStats` `TotalCodewords`, `CorrectedCodewords`, `UncorrectableCodewords` (Counter64), `...CtrDiscontinuityTime`. Key `ProfileId` 0..15, 255 for NCP [CM-OSSI §F.3.2.3.2.19].
- OFDM power: `docsIf31CmDsOfdmChannelPowerTable` (`...BandIndex`, `...CenterFrequency`, `...RxPower` tenth dBmV). Per 6 MHz band at the F connector, centers 111 + 6(n−1) MHz, index 1 to 33 by frequency, 0 for the PLC band;
  bands with no active subcarriers are skipped [CM-OSSI §F.3.2.3.2.32 to .36, PHY §9.3.9].
- Band plan and version: `docsIf31CmSystemCfgStateDiplexerCfgBandEdge`, `...CfgDsLowerBandEdge`, `...CfgDsUpperBandEdge`; `docsIf31DocsisBaseCapability`.

Only counter differences have meaning; resets show in `ifCounterDiscontinuityTime` and `sysUpTime` [CM-OSSI §8.4.1]. `docsIfCmStatus*` and `docsIfSigQSignalNoise` are deprecated on D3.1 CMs [CM-OSSI §A.1].

**CMTS CM status** [CCAP-OSSI §7.2.2.2, §A.1] [normative]. `docsIf3CmtsCmRegStatusId` is the per-CM key, equal to the older `docsIfCmtsCmStatusIndex`; that older table is deprecated [CCAP-OSSI §7.1.1.5.3].

| Table | Fields |
|---|---|
| `docsIf3CmtsCmRegStatusTable` | `MacAddr`, `IPv4Addr`, `IPv6Addr`, `Value`, `MdIfIndex`, `MdCmSgId`, `RcsId`, `TcsId`, `LastRegTime` |
| `docsIf31CmtsCmRegStatusTable` | `DsProfileIdList`, `UsProfileIucList`, `TcsPhigh`, `TcsDrwTop` (quarter dBmV), usable frequency limits (Hz), `PartialSvcState`, `PartialChanState` |
| `docsIf3CmtsCmUsStatusTable` (CM, channel) | `RxPower` (tenth dBmV, total), `SignalNoise` (tenth dB), `Microreflections` (dBc), `EqData`, `Unerroreds`, `Correcteds`, `Uncorrectables`, `HighResolutionTimingOffset`, `IsMuted`, `RangingStatus` |
| `docsIf31CmtsCmUsOfdmaChannelStatusTable` | `RxPower` (tenth dBmV per 1.6 MHz), `MeanRxMer`, `StdDevRxMer`, `ThresholdRxMerValue` (hundredth dB), `RxMerThreshold` (percentile), `ThresholdRxMerHighestFreq`, `Microreflections`, timing offset (19.0734 ps), `IsMuted`, `RangingStatus`, partial-service reason, time, count |
| `docsIf31CmtsCmUsOfdmaProfileStatusTable` (Data IUC) | `TotalCodewords`, `CorrectedCodewords`, `UnreliableCodewords` (Counter64), `PreFecErrorFreeCw`, `Timestamp` |
| `docsIf31CmtsCmDsOfdmChannelStatusTable` | Partial-service reason, time, counts; `PreferredProfile` |
| `docsIf31CmtsCmDsOfdmProfileStatusTable` | Partial-channel reason, time |

CmtsCmRegState differs from the CM's numbering [CCAP-OSSI §6.4.9]: other 1, initialRanging 2, rangingAutoAdjComplete 4, dhcpV4Complete 5, registrationComplete 6, operational 8, bpiInit 9, startEae 10,
startDhcpV4 11, startDhcpV6 12, dhcpV6Complete 13, startConfigFileDownload 14, configFileDownloadComplete 15, startRegistration 16, forwardingDisabled 17, rfMuteAll 18.

`PartialSvcState`: other 1, none 2, partialSvcDsOnlyImpaired 3, partialSvcUsOnlyImpaired 4, partialSvcDsAndUsImpaired 5 [CCAP-OSSI §6.4.35]. `PartialChanState` bits: fecErrorsDsProfile 0, fecErrorsNcpProfile 1, fecErrorsPlc 2; the text adds
lostFecLockUsProfile 3 [CCAP-OSSI §6.4.33]. Partial-service reasons include lostFecLock 2 and iuc13CwErrors 3, the latter vendor-defined [CCAP-OSSI §6.4.36]. The CM tables have no partial-service object; the CM logs events T07.0 and U102.0 [CM-OSSI Annex C].

**CMTS per channel.** `docsIf3CmtsSignalQualityExtCNIR` (tenth dB) and `...ExpectedRxSignalPower` (tenth dBmV), per logical upstream channel [CCAP-OSSI §6.6.1.2.1]. `docsIf3SignalQualityExtRxMER` (tenth dB), per logical upstream channel, averaged over
bursts that may be from several users [CCAP-OSSI §7.2.2.10.1]. Per-CM SNR differs per CM, unlike channel RxMER [CM-OSSI §I.1.2].

**IPDR.** Service definitions include CMTS-CM-REG-STATUS-TYPE, CMTS-CM-US-STATS-TYPE, CMTS-TOPOLOGY-TYPE and utilization [CCAP-OSSI Table 480]. Some upstream PNM use cases need IPDR, which forces integration or dual paths [PNM-HFC §8.4.3].

## 7. Mandatory and optional

- **D3.1 CM.** Must implement DOCS-IF3-MIB, DOCS-IF31-MIB, DOCS-PNM-MIB [CM-OSSI §7.1.3]. All objects in §1 and §6 are M [CM-OSSI §A.1]. Wideband capture is MUST; capturing the full downstream is SHOULD [PHY §9.3.2].
- **D3.1 CCAP.** All of §1 is M. `docsPnmCmtsUsSpecAn*` and the old bulk objects are D; `docsPnmCmtsOptProfChgCfgTable` is O [CCAP-OSSI §A.4]. UTSC minimum: timeIQ and fftPower, rectangular window plus Hann or Blackman-Harris; `docsPnmCmtsUtscCapabTable`
  reports what's supported; multiple simultaneous requests not required [CCAP-OSSI §7.3.5.6]. Zero-threshold impulse noise is MAY [PHY §9.4.3].
- **Before 3.1.** Pre-eq and FBC are SNMP via DOCS-IF3-MIB, without the file path [PNM-HFC §5.2.8.10, §6.2.6.5]. D3.0 DS equalization has compliance and interpretation problems; upstream spectrum beyond one channel was proprietary [PNM-HFC §1.2] [field]. Vendor spectrum OIDs exist
  under Broadcom's `1.3.6.1.4.1.4413.2.2.2.1.2.5` [PNMP §IX.3] [field].
- **Practice differs.** All known UTSC deployments need proprietary elements and none follow the spec [PNM-HFC §8.4.6] [field]. Not all capture modes exist, and files vary (one or many samples) [PNM-HFC §6.4.3] [field]. OFDMA RxMER bulk methods vary by vendor [PNM-HFC §6.3.1] [field].

## 8. Timing, limits and scale

| Item | Value | Source |
|---|---|---|
| CM concurrent tests; concurrent uploads | 1 covered test; 1 upload | CM-OSSI §D.2.3, §D.5.3.2.3 |
| CM file store | ≥ 4 files, ≥ 64 KB each | CM-OSSI §D.5.2 |
| Histograms (CM, CCAP) | ≥ 10 million samples per bin in ≤ 30 s; timeout default 1800 s | CM-OSSI §D.2.12, CCAP-OSSI §7.3.5.3 |
| Impulse noise | ≤ 5.12 MHz band; ring of 1024 events | CCAP-OSSI §7.3.5.2 |
| US RxMER averages | 1..255, leaky integrator | CCAP-OSSI §7.3.5.5.5 |
| NPR notch | Default 600 s; can cause link downtime; infrequent use | CCAP-OSSI §7.3.4.2 |
| UTSC | ≤ 100 kHz bins; free-run at supported rate | PHY §9.4.2 |
| CCAP concurrency | Vendor limits allowed | CCAP-OSSI §7.3.1 |

No source gives a rate limit in tests per second or a CM test duration.

Field and guideline:
- **SNMP cost.** SC-QAM pre-eq for a CM and CMTS pair is at least 614 bytes (208 in binary). 1 million devices is about 600 MB a poll, 5 million about 3 GB. At 250 ms round trip, 1 million serial queries take about 70 hours; across 1000 threads, under 5 minutes [PNM-HFC §6.2.6.4.1] [guideline].
- **Three tiers** [PNM-HFC §6.2.6.4.2] [guideline]. Low: daily, rotating 8-hour shifts, to catch time-of-day faults. Medium: every 4 hours for flagged modems, kept for 48 hours past threshold. High: every 10 minutes, critical devices, auto-expiring. Take CMTS coefficients close in time to the CM's by bulk walk. Reuse data rather than over-polling [PNM-HFC §6.2.6.4].
- **OFDMA RxMER** [PNM-HFC §6.3.1, §8.4.3] [field]. Operators want one sample per CM per hour, and many deployments can't. Some platforms do one modem at a time; some can't run RxMER and UTSC together; some CMTSs cause an out-of-service event, despite [PHY §9.2]. A wildcard MAC is a proposal [PNM-HFC §8.4.6], yet the §6.3.1 vendor recipe uses `FFFFFFFFFFFF`.
- **UTSC rates** [PNM-HFC §6.4.3 Table 23] [field]. Vendor SNMP: under 50 kbps, refresh 2 to 10 s, 50 to 300 kHz. TFTP: about 1 Mbps, refresh 200 ms to 1 s, 50 kHz (sub, mid-split) or 100 kHz (high-split), a 7 to 70 kB file each second, and less CMTS load than SNMP.
- **Blocking.** A slow request can block a modem from others; the guideline's example is five minutes [PNM-HFC §8.4.3] [field]. CCF's example waits 300 s after measurement before taking files [TR-XCCF §7.4.1.3] [field].
- **Conflict.** TR-XCCF §6.2 says 10-minute FEC samples are every 10 s; 600 samples and [CM-OSSI §D.2.10] mean 1 per second. Use OSSI.

## 9. Common Collection Framework

XCCF is a CableLabs reference for collecting once, serving many applications and hiding protocols behind REST [TR-XCCF §5.1, §5.2]. DCCF is its DOCSIS part, 3.1-focused and able to poll 3.0 and 2.0 [TR-XCCF §5.5, §7.1]. Protocols are SNMP and TFTP. IPDR, security, scale and persistence are out of scope [TR-XCCF §1.2, §7.2.5]. Components [TR-XCCF §5.3, §7.2]:
- **RAD**: REST API, job ids, file store.
- **WAC**: workflow controller, the only stateful element. Tracks CMTS and CM workload to prevent overload; instantiates drivers.
- **Drivers**: stateless operations, such as an SNMP GET or telling a CM where to send data.
- **Translator pool**: stateless format conversion. Raw data stays available.
- **Local cache**: filesystem with a directory per device. PNM data is binary via TFTP, MIB data is JSON.

Assumptions: one WAC per CMTS, each request for one CMTS, CMTS by IP, CMs by MAC [TR-XCCF §5.4]. Workflow: discover CMs from the CCAP by SNMP, then set each CM's TFTP destination [TR-XCCF §7.3.1]. Operators commonly run one collector per CMTS or core [PNM-HFC §8.4.3] [field].

## 10. Plant actives

**Transport.** SCTE 283 defines no communication method and sets no transponder requirements [SCTE 283 §1.2, §6.4]. Smart-amp proposals are HMS, LoRaWAN and G3-PLC behind a controller acting as proxy, plus an embedded DOCSIS CM that seems not to meet power and cost targets; per-amp addressing is still in work [PNM-HFC §8.8.9.2] [field]. HMS objects are SNMP-defined [SCTE 38-10 §Requirements, SCTE 38-5 §6].

**SCTE-HMS-RFAMPLIFIER-MIB**, relative to `rfAmplifierIdent` [SCTE 38-10 §Requirements] [normative]. The numeric root is in SCTE-HMS-ROOTS, not available here.

| Object | Node | Unit | Status |
|---|---|---|---|
| `rfAmpLinePowerVoltage1`, `...2` | 8, 9 | 1 VAC | optional |
| `rfAmpLinePowerCurrent` | 10 | 0.1 A, total draw | optional |
| `rfAmpDCPowerVoltage`, `...Current` (`rfAmpDCPowerTable` 14) | entry 2, 3 | 0.1 V signed; 0.1 A | mandatory; optional |
| `rfAmpRFActiveOutputLevel`, `...Current` (`rfAmpRFActiveTable` 3) | entry 3, 4 | 0.1 dBmV of a pilot; mA | optional |
| `rfAmpRFActiveControlType`, `...ControlLevel` | entry 2, 5 | none, alc, asc, agc, alsc; 0.1 VDC | optional |
| `rfAmpRFPortOutputRFLevel` (`rfAmpRFPortTable` 6) | entry 4 | 0.1 dBmV of a pilot, per port | optional |
| `rfAmpRFPortReverseAttenuationControl` | entry 7 | low, high, pad | optional |

Unsupported objects are simply absent. **There is no temperature object and no overall status object.** Alarms are `hmsAlarmEvent` traps tied to entries in SCTE-HMS-PROPERTY-MIB.

**SCTE-HMS-FIBERNODE-MIB**, relative to `fnIdent` [SCTE 38-5 §6] [normative]. The same line-power objects at 14, 15, 16; `fnDCPowerTable` 19; `fnRFActiveTable` 8; `fnRFPortTable` 11. Node-specific: `fnOpticalReceiverPower` (mandatory, 0.1 mW), `...State`, `...Current` (mA); `fnReturnLaserOpticalPower` (mandatory, 0.1 mW), `fnReturnLaserCurrent` (mA), `fnReturnLaserTemp` (optional, 1 °C), `fnReturnLaserWavelength` (0.01 nm); `fnOpticalReceiverABSwitchState`. The only temperature is the return laser's.

**Smart amplifier, SCTE 283** [normative]. An information model for SCTE 279 amps; protocol-specific data models are separate [SCTE 283 §1.2, §6.2].
- `Sensor`: `Id`, `Name`, `SensorType` (voltsAC 3, voltsDC 4, amperes 5, watts 6, hertz 7, celsius 8, percentRH 9), `Scale`, `Precision`, `Value`, `OperStatus`, `ValueTimeStamp`, `ValueUpdateRate` (ms). Read Value only with Type, Scale and Precision; Precision 1 and Scale units is °C × 10 [SCTE 283 §7.1.1.8].
- `PowerSupply`: `InputVoltage` (0.1 V RMS), `InputCurrent` (0.1 A RMS), `OperStatus` (other 0, unknown 1, up 2, down 3). Zero means unknown or unsupported [SCTE 283 §7.1.1.9]. `OutputRail`: `Voltage`, `Current` [SCTE 283 §7.1.1.10].
- `DsLogicalPortStatus`: `ReportedOutputPower` (tenth dBmV, total composite over occupied spectrum, 0 if unknown), `DsAgcEnabled`, `DsAgcToUpperLimit`, `DsAgcToLowerLimit` (tenth dB) [SCTE 283 §7.2.2.5]. `UsLogicalPortStatus`: `RfLevelControl` and its limits [SCTE 283 §7.2.2.4].
- `TestPointStatus`: `PointFreq` (MHz), `PointLevel` (tenth dBmV at the test point, −20 dB relative to the port) [SCTE 283 §7.2.2.10].
- Spectrum capture (optional): CM-like attributes per port and direction, file 504E4E09 or 504E4D09, amplitude referred to the physical port, TFTP [SCTE 283 §7.1.10, §7.4]. `SupportsRfSpectrumCapture` gives port bits [SCTE 283 §7.2.1.1.4].
- Events: temperature (O101.00 to .03), excess input current, improper input or output voltage, door [SCTE 283 Annex A].

PNM-HFC §8.3.3 calls spectrum capture the only telemetry SCTE 283 specifies, although the model has sensors. Read that as RF test data [inferred].

## Implications for hfc-twin

All [inferred]. Invariant 2 stands: this maps real data into the schema and does not extend it because a vendor exposes something.

| §9 field | Real source | Conversion and caveat |
|---|---|---|
| `rxmer_db[subcarrier]` | File 504E4E04 | / 4; 0xFF to `None`; frequency from header |
| `rxmer_mean_db`, `rxmer_stddev_db` | `docsPnmCmDsOfdmRxMerMean`, `...StdDev` | / 100; no file needed |
| `rxmer_p2_db` | `docsPnmCmDsOfdmRxMerThrVal`, `Percentile` 2 | / 4; same floor rule as the §5.5 oracle |
| `fbc_dbmv[bin]` | File 504E4E09 or `docsIf3CmSpectrumAnalysisMeasTable` | / 100; per RBW, not 6 MHz; needs calibration; set First and Last, as defaults stop at 993 MHz; band may be partial |
| `preeq` 24 taps | `docsIf3CmStatusUsEqData` | Vendor scale; `mtc_db`, `mtr_db`, `nmter_db` are adapter-derived |
| `preeq` OFDMA, `ResponseSummary` | File 504E4E06 (s2.13); `docsPnmCmUsPreEq*` | / 1000; files omit the summary, so read both or recompute |
| `ds_chan_est[channel]`, `ResponseSummary` | File 504E4E02 (0x8000 to `None`); `docsPnmCmOfdmChEstCoef*` | `mag_*` from Amp*, `gd_*` from GrpDelay*, / 1000 |
| `ds_rx_dbmv[channel]` | SC-QAM `docsIfDownChannelPower`; OFDM `docsIf31CmDsOfdmChannelPowerRxPower` | / 10. **OFDM gives up to 33 bands plus PLC, not one per channel** |
| `us_tx_dbmv` OFDMA | `docsIf31CmUsOfdmaChanTxPower` | / 4, per 1.6 MHz; a target, not a measurement |
| `us_tx_dbmv` SC-QAM | `docsIf31CmUsScQamChanTxPsd`; `docsIf3CmStatusUsTxPower` | / 4 per 1.6 MHz; / 10 total. **Total is always total on a 3.1 CM, so §9's "per 1.6 MHz if both 3.1" holds only for TxPsd** |
| `codewords` OFDM | `docsIf31CmDsOfdmProfileStats*` | Counter64 deltas; or file 504E4E08 for 1 s or 1 min history |
| `codewords` SC-QAM | `docsIfSigQExt*` | total = unerrored + corrected + uncorrectable; deltas |
| `plc`, `ncp` | `docsIf31CmDsOfdmChan` counters | Deltas; exact match |
| `partial_service` | `docsIf31CmtsCmRegStatusPartialSvcState` | CMTS only; `none` is false; loses direction |
| `t3_timeouts`, `t4_timeouts` | `docsIf3CmStatusUs*`, `docsIf31CmStatusOfdmaUs*` | **CM only, per US channel, cumulative**; sum deltas |
| `us_snr_db[channel]` | `docsIf3SignalQualityExtRxMER` or CNIR; per-CM `docsIf3CmtsCmUsStatusSignalNoise` | / 10; channel value fits physics; real SNR varies per CM; OFDMA unclear (Gaps) |
| `us_rxmer_db[subcarrier]` | File 504E4E69, per CM | Expensive (§8); `docsIf31CmtsCmUsOfdmaChannelMeanRxMer` is cheap |
| Active `v_in` | 283 `InputVoltage`; `rfAmpLinePowerVoltage1` | / 10; as is. Optional in HMS |
| Active `i_draw` | 283 `InputCurrent`; `rfAmpLinePowerCurrent` | / 10. Optional in HMS |
| Active `temp_c` | 283 `Sensor` type celsius | **No HMS amp object**; node has laser temperature only |
| Active `out_level_dbmv(f_hi)` | Nothing direct | **HMS gives a pilot; 283 gives total composite, or a test point at its own frequency, −20 dB.** Redefine it or fit through a known plan |
| Active `status` | 283 `OperStatus`; HMS traps | **HMS has no status object**; derive from reachability and alarms |
| Active `out_levels[port]` (observation version 2, flag `out_level_per_port`) | `rfAmpRFPortOutputRFLevel` (HMS amp, per RF port) | / 10; a pilot's level, optional; added because the sim computes each output's level (§15) |
| Service group `optics.node_rx_dbm` (flag `node_optical_rx`) | Analog node: `fnOpticalReceiverPower` (HMS fiber node, 0.1 mW). RPD: its SFP+ receive power, in SFF-8472's real-time diagnostics (A2h bytes 96-111, 0.1 µW), as R-OSSI's `SfpPlusStatus` carries it (fiber-optics.md §8) | 10·log10 of mW; at 0.1 mW a link below about −13 dBm reads 0, so the sim reports nothing there (fiber-optics.md §8) |
| Service group `optics.hub_rx_dbm` (flag `hub_optical_rx`) | Hub return receiver `heOpRxInputPower` (SCTE 85-2, 0.1 dBm); a Core's hub SFP+ receive power, SFF-8472 | As is; to 0.1 dB |
| Service group `optics.rpd_optic` (flag `rpd_optic_status`) | The RPD's SFP+ diagnostics: Tx and Rx power (A2h bytes 96-111) and alarm flags (112-113), via `SfpPlusStatus` (fiber-optics.md §8) | `link_up` from receive power between the module's sensitivity and overload, as §16 reads it |

**Adapter design.**
1. Most modem fields need direct SNMP to the CM and a TFTP path from CM subnets. A CMTS-only deployment keeps only CMTS tables, CCAP tests and OPT MER. Model access path as a dimension.
2. Budget at four scopes: CM (one test, one upload, four files), CCAP (vendor limits), TFTP server, SNMP. Keep summary reads apart from test slots, and use them to choose which modems get files. That's the tier model, and it fits active sensing.
3. Keep raw files, then parse. Branch on prefix and major. Treat `sampleTruncated`, `interfaceModification` and missing files as normal outcomes, and retry at the application layer.
4. Invariant 10: default file names and file headers carry the CM MAC, pre-eq also the CMTS MAC. Set our own `FileName`, hash at ingest, and treat the TFTP drop zone as sensitive.
5. Use header capture time, or per-set FEC timestamps, for `t`. A status read and its file are taken at different times.

**Capability masks.** Separate "mandatory by version" from "observed on this device"; UTSC and bulk RxMER are M on paper and vary in practice, so probe per model.
- Defaults by version: D3.1 turns on all CM PNM items; D3.0 gives SC-QAM pre-eq and SNMP FBC only. Optional items stay off until probed.
- Split RxMER, channel estimate and OFDMA pre-eq into file and summary. Split per-profile FEC counters from the history file. Split FBC by file or SNMP and record the covered band.
- Split `active transponder fields` into `v_in`, `i_draw`, `temp_c`, `out_level`, `status`, by source. All are optional or absent in HMS.
- Add upstream RxMER summary separate from per-subcarrier, UTSC from `docsPnmCmtsUtscCapab*`, 6 MHz band power, and access path (CM SNMP, CM TFTP, CMTS SNMP, IPDR).

**Candidate fields**, only if the sim can produce them: registration state; CMTS-received power per CM (`docsIf3CmtsCmUsStatusRxPower`, `docsIf31CmtsCmUsOfdmaChannelRxPower`); upstream FEC, as §9 doesn't say whether `codewords` covers it; `RangingStatus`, `T3Exceededs`, `IsMuted`; `PreEqCoAdjStatus` (already deferred); `ThresholdRxMerHighestFreq`; SC-QAM DS RxMER; histograms for laser clipping.

## Gaps

- No DOCS-PNM-MIB or DOCS-IF31-MIB module text was available. OIDs for CM test tables, UTSC, DOCS-IF31 CM tables and CCAP bulk config aren't in any source and aren't guessed. Fetch from `mibs.cablelabs.com`.
- SCTE 37 (ROOTS), 38-1 (PROPERTY) and 38-2 (ALARMS) weren't provided, nor any common HMS MIB. So numeric HMS OIDs are missing, and whether another HMS MIB supplies amp temperature or status can't be settled.
- FEC set-count width and histogram length semantics are inconsistent in the spec. Check against real files.
- The registry [CANN] wasn't available, so the file type list here is only what the OSSIs state.
- Whether `docsIf3SignalQualityExtRxMER` or CNIR is instantiated for OFDMA channels isn't clear. No channel-wide OFDMA noise figure was found.
- `docsIf31CmUsScQamChanTxPsd` on a CM with a D3.0 CMTS: applicability unverified.
- `docsIfSigQExt*` definitions are in RFC 4546 or companions, not read. The `total` formula is our assumption.
- CM-OSSI §I.1.3 prints N+I as CNIR minus ExpectedRxSignalPower, which looks sign-reversed for an absolute level. Not used here.
- PNM-HFC cites pre-eq at CM-OSSI §D.2.12; in I27 it's §D.2.13. Section numbers here are I27.
- Not covered: MULPI OPT-REQ details, latency reports (`docsCmtsLatencyRptCfg*`), DOCS-LEAK-DETECT-MIB, SCTE 283 data models, and the CCAP v4.0 gNMI method. R-PHY details belong in `daa-remote-phy.md`.
- Test durations and safe per-device rates aren't in any source; measure them in the lab.
