# Reference library

Our own notes on how DOCSIS and HFC plant work, distilled from CableLabs and SCTE
documents. Load the file for the topic at hand instead of going back to the PDFs.

## What to load

| Working on | Load |
|---|---|
| Plant graph, cable, taps, amps, levels (steps 2, 3, 8, 9) | [plant-and-powering.md](plant-and-powering.md) |
| AC powering (step 3) | [plant-and-powering.md](plant-and-powering.md) |
| OFDM and OFDMA parameters, channel plans, profiles (steps 1, 3) | [phy-docsis31.md](phy-docsis31.md) |
| Downstream telemetry (steps 1, 4) | [pnm-downstream.md](pnm-downstream.md) |
| Upstream telemetry and pre-eq (steps 1, 4) | [pnm-upstream.md](pnm-upstream.md) |
| Fault signatures and diagnosis (steps 4, 5, 12) | [impairments.md](impairments.md) |
| RxMER curve shapes and field severity, the sim against them (steps 16, 17) | [rxmer-impairments.md](rxmer-impairments.md) |
| Modem state: ranging, timeouts, partial service (step 4) | [mac-operations.md](mac-operations.md) |
| Adapters, SNMP, PNM files, transponders (step 7, v2) | [collection-management.md](collection-management.md) |
| RPD and DAA (steps 10, 13) | [daa-remote-phy.md](daa-remote-phy.md) |
| DOCSIS 4.0, ESD and FDX, and what they require of anything built earlier | [docsis40.md](docsis40.md) |
| Design practice: tap levels, tap and amp families, cable by role, branching, node size, powering, structures, in-home wiring, upgrades (tasks 21, 23) | [design-practice.md](design-practice.md) |
| Fiber and optics: loss, splices, connectors, cable, analog and digital links, hub equipment, architectures (tasks 21, 22) | [fiber-optics.md](fiber-optics.md) |

`docs/physics.md` holds the equations the sim implements. These files explain the
systems behind it. Where they disagree, stop and resolve it; don't pick one silently.
The exception is **Implications for hfc-twin**, which records what each review found on
2026-11-02. Much of it has since been applied, so `docs/physics.md` and the task specs
win where those sections differ.

## Conventions

Each file is kept to about 400 lines so it stays cheap to load. It ends with
**Implications for hfc-twin** and **Gaps**.

Every claim cites its source as `[DOC §section]`, using the short names below, and
carries one of these tags when it matters:

- **[normative]**: a requirement in a specification.
- **[guideline]**: recommended practice or a suggested threshold.
- **[field]**: an example or observation from operators or labs, not a general rule.
- **[inferred]**: our own conclusion. Never present these as coming from the source.

Units follow `CLAUDE.md`. Convert source values and say so when a source uses
something else.

These are notes in our own words. Facts such as numeric limits, formulas, field names
and MIB identifiers are recorded; prose and whole tables are not copied. Source PDFs
are not committed. Fetch them from the [CableLabs document library](https://library.cablelabs.com/).

## Sources

Revisions as listed in the library on 2026-11-02.

| Short name | Document | Revision |
|---|---|---|
| PNM-HFC | PNM Methods and Practices in HFC Networks | CM-GL-PNM-HFC-V02-260130 |
| PNM-3.1 | PNM Current Methods and Practices in HFC Networks (DOCSIS 3.1) | CM-GL-PNM-3.1, rev 05, 2023-09-27 |
| PNMP | PNM Best Practices: HFC Networks (DOCSIS 3.0) | CM-GL-PNMP-V03-160725 |
| PHY | DOCSIS 3.1 Physical Layer Specification | CM-SP-PHYv3.1-I20-230419 |
| MULPI | DOCSIS 3.1 MAC and Upper Layer Protocols Interface | CM-SP-MULPIv3.1-I25-230419 |
| CM-OSSI | DOCSIS 3.1 Cable Modem OSSI | CM-SP-CM-OSSIv3.1, I27, 2025-02-19 |
| CCAP-OSSI | DOCSIS 3.1 CCAP OSSI | CM-SP-CCAP-OSSIv3.1, I30, 2026-04-29 |
| R-PHY | Remote PHY Specification | CM-SP-R-PHY, I22, 2026-08-19 |
| TR-DCA | Distributed CCAP Architectures Overview | CM-TR-DCA-V01-150908 |
| TR-MIDSPLIT | Midsplit Migration Implications on the HFC Network | CM-TR-MID-SPLIT, I01, 2015-02-23 |
| TR-PMA | DOCSIS 3.1 Profile Management Application | CM-TR-PMA, 01, 2018-05-30 |
| TR-XCCF | Combined Common Collection Framework Architecture | CL-TR-XCCF-PNM, 01, 2018-08-14 |
| SCTE 183 | SCTE Measurement Recommended Practices for Cable Systems | 2023 |
| SCTE 297 | Pre-Equalization Based Upstream Frequency Response Measurements | 2025r1 |
| SCTE 285 | DOCSIS 3.1 RxMER PNM Test Validation | 2023 |
| SCTE 258 | DOCSIS 3.1 Downstream OFDM Power Definition, Calculation, and Measurement | 2026 |
| SCTE 257 | Accurate Measurement of Downstream RF Signal Levels | 2024 |
| SCTE 209 | UHF Leakage, Ingress, Direct Pickup | 2024 |
| SCTE 205 | Outside Plant Power Recommended Preventive Maintenance | 2022 |
| SCTE 32 | Ampacity of Coaxial Telecommunications Cables | 2016 (R2021) |
| SCTE 15 | Specification for Trunk, Feeder and Distribution Coaxial Cable | 2022 |
| SCTE 74 | Braided 75 Ohm Flexible RF Coaxial Drop Cable | 2023 |
| SCTE 40 | Digital Cable Network Interface Standard | 2016 (R2021) |
| SCTE 38-10 | SCTE-HMS-RF-AMPLIFIER-MIB | 2017 (R2022) |
| SCTE 38-5 | SCTE-HMS-FIBERNODE-MIB | 2017 (R2022) |
| SCTE 283 | Information Model for Smart Broadband Amplifiers | 2025 |
| PHY4 | DOCSIS 4.0 Physical Layer Specification | CM-SP-PHYv4.0, I08, 2026-09-28 |
| MULPI4 | DOCSIS 4.0 MAC and Upper Layer Protocols Interface | CM-SP-MULPIv4.0, I12, 2026-10-14 |
| CM-OSSI4 | DOCSIS 4.0 Cable Modem OSSI | CM-SP-CM-OSSIv4.0, I14, 2026-10-14 |
| CCAP-OSSI4 | DOCSIS 4.0 CCAP OSSI | CM-SP-CCAP-OSSIv4.0, I14, 2026-10-14 |
| TR-TAR | Cable Operator Preparations for DOCSIS 4.0 Technology Deployment | CM-TR-TAR-4.0, 01, 2023-06-29 |
| SCTE 279 | 1.8 GHz Broadband RF Hardline Amplifiers for Cable Systems | 2022 |
| SCTE 296 | Full Duplex DOCSIS Broadband RF Hardline Amplifiers for Cable Systems | 2025r1 |
| SCTE 300 | Best Practices for Migrating to DOCSIS 4.0 | 2024 |
| VIAVI2016 | VIAVI, [DOCSIS 3.1: Best Practices for Peak Performance](https://www.normann-engineering.com/support/white_papers/docsis-31-best-practices-peak-performance-white-paper-en.pdf), vendor white paper | 2016 |
| ITU-T G.652 | [Characteristics of a single-mode optical fibre and cable](https://www.itu.int/rec/T-REC-G.652-202408-I) | 08/2024 |
| ITU-T G.657 | [Bending-loss insensitive single-mode fibre and cable](https://www.itu.int/rec/T-REC-G.657-202408-I) | 08/2024 |
| ITU-T G.694.1 | [Spectral grids for WDM: DWDM frequency grid](https://www.itu.int/rec/T-REC-G.694.1-202010-I) | 10/2020 |
| ITU-T G.694.2 | [Spectral grids for WDM: CWDM wavelength grid](https://www.itu.int/rec/T-REC-G.694.2-200312-I) | 12/2003 |
| ITU-T G-Sup39 | [Optical system design and engineering considerations](https://www.itu.int/rec/T-REC-G.Sup39-202510-I) | 10/2025 |
| ITU-T J.186 | [Transmission equipment for multi-channel TV over optical access networks by SCM](https://www.itu.int/rec/T-REC-J.186-200806-I) | 06/2008 |
| IEEE 802.3 Cl.52 | IEEE Std 802.3-2008 Section Four, Clause 52 (10GBASE-S/L/E PMDs), free via [IEEE GET](https://ieeexplore.ieee.org/browse/standards/get-program/page/series?id=68). The copy read was hosted outside IEEE | 2008, third printing 2010-06-22 |
| SFF-8472 | [Management Interface for SFP+](https://www.snia.org/sff/specifications) | Rev 12.5a, 2026-01-16 |
| SCTE 270 | [Mathematics of Cable](https://account.scte.org/documents/5880/SCTE_270_2021r1.pdf) | 2021r1 |
| SCTE 181 | [Recommended Practices for Optical Fiber Construction and Testing](https://account.scte.org/documents/8117/SCTE_181_2025.pdf) | 2025 |
| SCTE 86 | [Optical Fiber Cable Types for OSP Trunk and Distribution](https://account.scte.org/documents/3625/ANSI_SCTE_86_2021.pdf) | 2021 |
| SCTE 174 | [RFoG FTTH System Specification](https://account.scte.org/documents/7952/ANSI_SCTE_174_2018_R2024.pdf) | 2018 (R2024) |
| SCTE 240 | [Test Procedures for Testing CWDM Systems](https://account.scte.org/documents/5088/ANSI_SCTE_240_2017_R2022.pdf) | 2017 (R2022) |
| SCTE 85-1 | [HMS HE Optical Transmitter MIB](https://account.scte.org/documents/5697/ANSI_SCTE_85-1_2017_R2022.pdf) | 2017 (R2022) |
| SCTE 85-2 | [HMS HE Optical Receiver MIB](https://account.scte.org/documents/5700/ANSI_SCTE_85-2_2017_R2022.pdf) | 2017 (R2022) |
| SCTE 85-3 | [HMS HE Optical Amplifier MIB](https://account.scte.org/documents/5888/ANSI_SCTE_85-3_2017_R2022.pdf) | 2017 (R2022) |
| FOA-LB | Fiber Optic Association, [Calculating Fiber Optic Loss Budgets](https://www.thefoa.org/tech/lossbudg.htm) | (c) 2004-18 |
| FBA-2025 | FBA and Cartesian, [Fiber Deployment Cost Annual Report 2025](https://fiberbroadband.org/wp-content/uploads/2026/01/FBA_Cartesian_Fiber-Deployment-Cost-Annual-Report_2025.pdf) | Jan 2026 |
| CS-OM6FDRX | CommScope, [OM6000 1.2 GHz Fiber Deep Forward Receiver](https://www.commscope.com/globalassets/digizuite/286513-om6-fd-receiver.pdf), data sheet | 1513488 RevB, 09-2021 |
| CS-OM4120 | CommScope, [OM4120 1.2 GHz 4x4 HFC Segmentable Node](https://www.commscope.com/globalassets/digizuite/62045-om4120.pdf), data sheet | 1512139 Rev H, 11-2021 |
| CS-HT3562H | CommScope, [CH3000 HT3562H DWDM Transmitter](https://www.commscope.com/globalassets/digizuite/61942-ht3562.pdf), data sheet | RevJ, 2022 |
| CS-FA3500 | CommScope, [CH3000 FA3500 Optical Amplifiers](https://www.commscope.com/globalassets/digizuite/61878-fa3500.pdf), data sheet | RevY, 08-2021 |
| CS-RP4111 | CommScope, [RP4111 RPD](https://www.commscope.com/globalassets/digizuite/294779-rp4111.pdf), data sheet | 1512604 RevC, 2022 |
| CS-OM6RPD | CommScope, [RPD for OM6000 HFC](https://www.commscope.com/globalassets/digizuite/565655-om6hfc-rpd.pdf), data sheet | 1513968 RevA, 08-2021 |
| CS-FOSC450 | CommScope, [FOSC 450 D6 splice closure](https://www.commscope.com/product-type/hubs-closures-terminals-boxes/fiber-closures/fiber-splice-closures/item760252273/), product page | read 2026-11-10 |
| CS-SCAPC | CommScope, [SC/APC connector](https://www.commscope.com/product-type/connectors/fiber-adapters-connectors/connectors/item860648957/), product page | read 2026-11-10 |
| AFL-LEDJ | AFL, [Non-Armored Loose Tube Double Jacket](https://aflglobal.com/-/media/Project/AFL-Global/Product-Specification-Sheet/fiberOpticCable/Double-Jacket-No-Armor.pdf), data sheet | PP-1-00015 Rev 7, 2020-04-17 |
| AFL-ADSS | AFL, [ADSS Standard Fiber Optic Cable](https://www.aflglobal.com/-/media/Project/AFL-Global/Product-Specification-Sheet/fiberOpticCable/ADSS-Standard-Fiber-Optic-Cable.pdf), data sheet | PP-1-00022 Rev 12, 2024-12-11 |
| FW-CMUX18 | Fiberworks, [F-CMUX-18 CWDM mux/demux](https://www.fiberworks.no/prodDoc/Fiberworks-muxes/F-CMUX-18%20Data%20Sheet%201.1.pdf), data sheet | v1.1, 2023-03-20 |
| CISCO-SFP10G | Cisco, [10GBASE SFP+ Modules](https://www.cisco.com/c/en/us/products/collateral/interfaces-modules/transceiver-modules/data_sheet_c78-455693.html), data sheet | updated 2026-10-06 |
| CISCO-DWDM-I | Cisco, [Industrial Temp 10GBASE DWDM SFP+](https://www.cisco.com/c/en/us/products/collateral/interfaces-modules/dwdm-transceiver-modules/datasheet-c78-741702.html), data sheet | updated 2025-01-09 |
| CISCO-SPHY | Cisco, [Smart PHY 300/600 Shelves](https://www.cisco.com/c/en/us/products/collateral/video/cbr-series-converged-broadband-routers/datasheet-c78-739789.html), data sheet | 2019-02-25 |
| CISCO-cBR-RPLC | Cisco, [cBR 40G Remote PHY Line Card](https://www.cisco.com/c/en/us/products/collateral/video/cbr-series-converged-broadband-routers/datasheet-c78-738847.html), data sheet | 2017-04-20 |
| CISCO-N9300FX3S | Cisco, [N9300-FX3S Series Switch](https://www.cisco.com/c/en/us/products/collateral/switches/nexus-9000-series-switches/datasheet-c78-744874.html), data sheet | updated 2026-10-09 |
| OSA5420 | Oscilloquartz, [OSA 5420 Series PTP Grandmaster](https://navi.pl/wp-content/uploads/2016/04/1447064244-2015-11_DS_OSA5420_Series.pdf), data sheet hosted by a distributor | 2015-11 |
| SCTE 264 | [Broadband RF Hardline Taps for Cable Systems](https://account.scte.org/documents/3678/ANSI_SCTE_264_2020.pdf) | 2020 |
| SCTE 265 | [Broadband RF Hardline Passives for Cable Systems](https://account.scte.org/documents/4869/ANSI_SCTE_265_2021.pdf) | 2021 |
| SCTE 153 | [Drop Passives: Splitters, Couplers and Power Inserters](https://account.scte.org/documents/4993/ANSI_SCTE_153_2021.pdf) | 2021 |
| SCTE 161 | [Drop Amplifiers](https://account.scte.org/documents/6595/ANSI_SCTE_161_2022.pdf) | 2022 |
| SCTE 129 | [Drop Passives: Bonding Blocks (Without Surge Protection)](https://account.scte.org/documents/3676/ANSI_SCTE_129_2021.pdf) | 2021 |
| SCTE 151 | [Requirements for RF Traps and Filters](https://account.scte.org/documents/4868/ANSI_SCTE_151_2021.pdf) | 2021 |
| SCTE 180 | [Recommended Practices for Coaxial Cable Construction and Testing](https://account.scte.org/documents/8352/SCTE_180_2025.pdf) | 2025 |
| SCTE 287 | [Right-Sizing Outside Plant Power Supplies](https://account.scte.org/documents/6579/SCTE_287_2023.pdf) | 2023 |
| SCTE 288 | [Plug-in Cable Attenuator Performance Standard](https://account.scte.org/documents/8064/SCTE_288_2024.pdf) | 2024 |
| SCTE 235 | [Coexistence of DOCSIS 3.1 and MoCA Signals in the Home](https://account.scte.org/documents/776/SCTE20235202017.pdf) | 2017 |
| SCTE 77 | [Specification for Underground Enclosure Integrity](https://account.scte.org/documents/6916/ANSI_SCTE_77_2023.pdf) | 2023 |
| SCTE 150 | [Preparing a Line Extender Specification](https://account.scte.org/documents/166/ANSI_SCTE-150-2013-1576860225162.pdf) | 2013 |
| VOGEL2019 | [M. Vogel, HFC Spectrum Expansion: Design and Component Impacts, SCTE•ISBE/NCTA paper](https://wagtail-prod-storage.s3.amazonaws.com/documents/735_Vogel_HFC_Spectrum_Expansion_Design_and_Component_Impacts_paper.pdf) | 2019 |
| ULM2016 | [Ulm and Maricevic, Future Directions for Fiber Deep HFC Deployments, vendor white paper](https://webresources.commscope.com/download/assets/Future+Directions+for+Fiber+Deep+HFC+Deployments/871b25b43bd811f0b87efaa31792deb3) | 2016 |
| CS-TD-MANUAL | [CommScope, Broadband Applications and Construction Manual, Trunk and Distribution](https://www.commscope.com/globalassets/digizuite/3447-broadband-app-const-manual-ii-106549-en.pdf) | II-106549.4-EN, 07/20 |
| CS-DROP-MANUAL | [CommScope, Broadband Applications and Construction Manual, Drop](https://www.commscope.com/globalassets/digizuite/2422-drop-const-manual-co-107145.pdf) | CO-107145.1-EN, 01/17 |
| CS-COAX-QRG | [CommScope, Coaxial Cables Quick Reference Guide](https://de.commscope.com/globalassets/digizuite/2253-coaxial-cables-quick-reference-guide-co-112310-en.pdf) | BR-112310.1-EN, 05/19 |
| TFC-CABLE-BOOK | [Times Fiber Communications, The Cable Book, with Technical Notes, on a third-party host](https://unexpocom.wordpress.com/wp-content/uploads/2010/01/tfc_cable_book_iii.pdf) | pages dated Apr 06 |
| RUS 1751F-626 | [USDA RUS, Staking of Aerial Plant](https://www.rd.usda.gov/files/UTP_Bulletins_1751F-626.pdf) | 1994-05-04 |
| RUS 1751F-630 | [USDA RUS, Design of Aerial Plant](https://www.rd.usda.gov/files/UTP_Bulletins_1751F-630.pdf) | 1996-01-19 |
| RUS 1751F-635 | [USDA RUS, Aerial Plant Construction](https://www.rd.usda.gov/files/UTP_Bulletins_1751F-635.pdf) | 1996-06-12 |
| RUS 1751F-643 | [USDA RUS, Underground Plant Design](https://www.rd.usda.gov/files/UTP_Bulletins_1751F-643.pdf) | 2002-08-28 |
| RUS 1753F-150 | [USDA RUS, Construction of Direct Buried Plant](https://www.rd.usda.gov/files/UTP_Bulletins_1753F-150.pdf) | 2010-09-30 |
| RUS 1753F-151 | [USDA RUS, Construction of Underground Plant](https://www.rd.usda.gov/files/UTP_Bulletins_1753F-151.pdf) | 2001-09-17 |
| RUS 1753F-153 | [USDA RUS, Service Installation at Customer Access Locations](https://www.rd.usda.gov/files/UTP_Bulletins_1753F-153.pdf) | 2001-09-17 |
| RUS 1753F-801 | [USDA RUS, Standard for Service Installations at Customer Access Locations](https://www.rd.usda.gov/files/UTP_Bulletins_1753F-801.pdf) | 2001-09-17 |
| WPS-JU10 | [Wisconsin Public Service, Joint Use Clearances JU10, a utility's pole attachment standard](https://www.wisconsinpublicservice.com/joint-use-management/ju-standards/joint-use.pdf) | 2020-10-01 |
| CLARK-JU2 | [Clark Public Utilities, Construction Standards §1910 Joint Use](https://www.clarkpublicutilities.com/wp-content/uploads/2018/02/1910-Joint-Use.pdf) | JU2 rev 3, 2023-03-07 |
| TPU-C-OH-1060 | [Tacoma Power, Pole Attachment Requirements for Telecommunications](https://www.mytpu.org/wp-content/uploads/C-OH-1060-Attach-Reqs-for-Telecom-240628.pdf) | 2026-02-12 |
| FCC 99-304 | [FCC Tenth Report and Order, universal service cost model inputs](https://docs.fcc.gov/public/attachments/FCC-99-304A1.pdf) | 1999-11-02 |
| FCC-CACM-2013 | [FCC WCB, Connect America Cost Model overview, v3.2](https://docs.fcc.gov/public/attachments/DOC-323344A1.pdf) | 2013-09-12 |
| OBI-TP1 | [FCC Omnibus Broadband Initiative, The Broadband Availability Gap, Technical Paper No. 1](http://download.broadband.gov/plan/the-broadband-availability-gap-obi-technical-paper-no-1.pdf) | 2010-04 |
| CS-FFTQ | CommScope FFT2/4/8-*Q 1.2 GHz taps, [data sheet](https://ja.commscope.com/globalassets/digizuite/61887-fftq.pdf) | 1514660 Rev B, 2-2023 |
| TX-OTTZ | Technetix OTTZ-2, -4 and -8 taps, [data sheet](https://americas.technetix.com/wp-content/uploads/sites/7/2025/07/19014593_OTTZ-2-xx_V2_DS_EN.pdf) | 2023-2026 |
| TEL-4RTP | Teleste 4RTPx 1.8 GHz rotatable taps, [data sheet](https://www.teleste.com/wp-content/uploads/2022/11/Teleste_Datasheet_Outdoor-Passives_Rotatable-taps_20221110.pdf) | 2022-11-10 |
| ANX-MHT | Antronix MHT2000 Milenium NXT taps, on a distributor's host, [data sheet](https://www.tvcinc.com/shared-downloads/7365300485-antronix-milenium-nxt-multi-taps.pdf) | DS-1230-TM-A01, 10/2021 |
| ATX-ANW1544 | ATX GigaXtend XST taps advisory, [data sheet](https://atx.com/wp-content/uploads/ANW1544-Product-Advisory-New-GigaXtend-XST-Taps.pdf) | Rev 07/2021 |
| TX-XFOMZ | Technetix XFOMZ faceplate, [data sheet](https://americas.technetix.com/wp-content/uploads/sites/7/2024/03/19014894_XFOMZ_V2_DS_AE.pdf) | 02/2024 V2 |
| CS-TQ | CommScope T-*-Q tap plug-ins, [data sheet](https://es.commscope.com/globalassets/digizuite/62385-tq-taps.pdf) | 1514480 Rev A, 11-2021 |
| TX-TXPI | Technetix TXPI-EQZ and CSZ, [data sheet](https://emea.technetix.com/wp-content/uploads/sites/2/2026/04/19014812_TXPI-xxx-xx_V2_DS_EN.pdf) | 03/2026 V2 |
| CS-SSPQ | CommScope SSP-Q line passives, [data sheet](https://de.commscope.com/globalassets/digizuite/62329-ssp-q.pdf) | 1514712 Rev A, 4-2022 |
| CS-FFE120 | CommScope FFE-120 feeder equalizer, [data sheet](https://de.commscope.com/globalassets/digizuite/61885-ffe120.pdf) | 1514662 Rev A, 3-2022 |
| TX-LH100 | Technetix LH100 1.8 GHz passives, [data sheet](https://emea.technetix.com/wp-content/uploads/sites/2/2024/04/LH100_1.8GHz_series_V2_DS_EN.pdf) | 04/2024 V2 |
| TX-XG | Technetix XG 1.8 GHz passives, [data sheet](https://americas.technetix.com/wp-content/uploads/sites/7/2023/08/19013831_XGx_V12_DS_EN.pdf) | 08/2023 V12 |
| TEL-4XP | Teleste 4xP-SA 1.8 GHz passives, [data sheet](https://www.teleste.com/wp-content/uploads/2022/11/Teleste_Datasheet_4xP-SA_1.8GHz-Outdoor-Passives_SA_20221108.pdf) | 2022-11-08 |
| CS-NPB | CommScope NPB 1.2 GHz pads, [data sheet](https://webresources.commscope.com/download/assets/NPB-%2A%2A0+1.2+GHz+Attenuator+Pads+Data+Sheet/5074163a3bd611f099f1068e18034550) | 7858851 Rev A, 8-2023 |
| CS-CE120 | CommScope CE-120 and CS-120, [data sheet](https://fr.commscope.com/globalassets/digizuite/472936-ce120-cs120.pdf) | 1513825 Rev F, 11-2025 |
| CS-SRE | CommScope SRE return equalizer, [data sheet](https://de.commscope.com/globalassets/digizuite/62327-sre.pdf) | 09-2021 |
| CS-SP120 | CommScope SP120 and DC120 plug-ins, [data sheet](https://commscope.com/globalassets/digizuite/580549-sp120-dc120.pdf) | 1513962 Rev B, 4-2022 |
| CS-BLE180 | CommScope BLE180 line extender, [data sheet](https://commscope.com/globalassets/digizuite/1001352-starline-ble180-1-8-ghz-line-extender.pdf) | Z1514848 Rev A, 6-2024 |
| CS-MB180 | CommScope MB180 MiniBridger, [data sheet](https://zh.commscope.com/globalassets/digizuite/1001353-starline-mb180-1-8-ghz-minibridger-amplifier.pdf) | Z1514847 Rev A, 6-2024 |
| CS-BLE120 | CommScope BLE120, [data sheet](https://de.commscope.com/globalassets/digizuite/330557-ble120.pdf) | 1514537 Rev D, 2-2023 |
| CS-MB120 | CommScope MB120, [data sheet](https://webresources.vistancenetworks.com/download/assets/STARLINE+MB120+1.2+GHz+Mini-Bridger+Amplifier+Data+Sheet/387b07b6f75e11f089076a092ffd85e3) | 1513666 Rev L, 2-2023 |
| TEL-ICON4000 | Teleste ICON4000 series, [data sheet](https://teleste.com/wp-content/uploads/2025/03/teleste-icon4000-series-product-brochure-2025.pdf) | 2025 |
| TX-DBT1800 | Technetix DBT-1800-C2, -C3 and -CC1, [data sheet](https://americas.technetix.com/wp-content/uploads/sites/7/2026/01/19014859_DBT-1800_GAINMAKER_C2_BRIDGER_V32_DS_AE.pdf) | V32-V35, 01/2026 |
| CISCO-HGBT | Cisco 1218 MHz GainMaker HGBT, archived copy, [data sheet](http://web.archive.org/web/20230607191834/https://www.cisco.com/c/en/us/products/collateral/video/gainmaker-amplifiers/datasheet-c78-736466.html) | 2019-03-27 |
| CISCO-HGD | Cisco 1218 MHz GainMaker HGD, archived copy, [data sheet](http://web.archive.org/web/20241009220947/https://www.cisco.com/c/en/us/products/collateral/video/gainmaker-amplifiers/datasheet-c78-736468.html) | 2019-06-12 |
| CS-NC2000 | CommScope NC2000, [data sheet](https://webresources.vistancenetworks.com/download/assets/NC2000+1.2+GHz+Scalable+Node+Platform+Data+Sheet/fd4b7d2cf75f11f0b79ae213ce2458ce) | 1511529 Rev C, 2-2023 |
| CS-NC4000S4 | CommScope NC4000S4, [data sheet](https://de.commscope.com/globalassets/digizuite/61990-nc4000s4.pdf) | 87-11085 Rev D, 01-2022 |
| CS-RD1424 | CommScope NC4000S4 RD1424-M4 RPD, [data sheet](https://webresources.commscope.com/download/assets/NC4000S4+RD1424-M4+Remote+PHY+Device+%28RPD%29+Data+Sheet/e6adf590f54811f0acda4abb6608f2ab) | 7865730 Rev A, 01-26 |
| VEC-EN9000 | Vecima Entra EN9000, [data sheet](https://vecima.com/wp-content/uploads/2024/05/EN9000-Entra-Access-Node.pdf) | 2024-05 |
| HAR-RIPPLE | Harmonic CableOS Ripple+, on a distributor's host, [data sheet](https://amt.com/wp-content/uploads/2024/10/Harmonic_DS_CableOS_Ripple.pdf) | 2021 |
| HAR-SHELL1 | Harmonic Shell-1, on a distributor's host, [data sheet](https://amt.com/wp-content/uploads/2024/06/AMT_Harmonic_DS_cOS-Shell-1-Remote-PHY-node_100423.pdf) | 2023-10-04 |
| ALPHA-XM31HP | Alpha (EnerSys) XM3.1-HP, [data sheet](https://www.enersys.com/498e33/globalassets/documents/product-documentation2/alpha/power-supplies/xm3.1hp/amer/amer-en-ds-alp-xm31hp.pdf) | AMER-EN-DS-ALP-XM31HP-0826 |
| ALPHA-XM3HP-TM | Alpha XM3-HP technical manual, [data sheet](https://enersys.com/4a2217/globalassets/documents/product-documentation2/alpha/power-supplies/xm3-hp/amer/EN-M-AMER-ALP-XM3-HP-017882B0.pdf) | 017-882-B0-001 Rev F, 05/2024 |
| ALPHACELL-HP | AlphaCell 3.5HP and 4.0HP, [data sheet](https://integration.enersys.com/4a31e4/globalassets/documents/product-documentation2/alpha/batteries/alphacell/hp/amer/alpha_092019_alphacellhp_datasheet.pdf) | 09/2019 |
| TX-TXUPS | Technetix TXUPS 900 and 1350, [data sheet](https://emea.technetix.com/wp-content/uploads/sites/2/2025/11/19016417_TXUPS_900_1350_V4_DS_EN-2.pdf) | 10/2025 V4 |
| ABS-T6 | Amphenol Broadband T6TSFx77-LTV-051M drop, [data sheet](https://www.amphenolbroadband.com/wp-content/uploads/2019/03/t6tsfx77-ltv_black_-051m_1.8ghz18.pdf) | 2018-12-12 |
| ABS-T11 | Amphenol Broadband T11TSFx77-LTV-072M drop, [data sheet](https://www.amphenolbroadband.com/wp-content/uploads/2019/03/t11tsfx77-ltv_black_-072m_1.8ghz18.pdf) | 2018-12-19 |
| ANX-A7BCP | Antronix A7BCP ground block, [data sheet](https://antronix.com/wp-content/uploads/2026/08/A7BCP_Ground_Block_with_CamPort.pdf) | 5/2026 |
| ANX-MRA | Antronix MRA1-15 and MRAH1-15-G2 drop amps, [data sheet](https://antronix.com/wp-content/uploads/2026/08/MRAH1-15-G2_Downstream_Gain_Amps.pdf) | DS-1243-AR-A01, 7/2022 |
| ANX-FRA | Antronix FRA1-15 drop amp, [data sheet](https://antronix.com/wp-content/uploads/2026/08/FRA1-15_Bi-Directional_Gain_Amps.pdf) | DS-1030-AR-A06 |
| ANX-GLF1002 | Antronix GLF-1002 point-of-entry filter, [data sheet](https://antronix.com/wp-content/uploads/2026/08/GLF-1002_Point_of_Entry_Low_Pass_Filter.pdf) | DS-1094-SN-A05, 6/2012 |
| ANX-CMC5000 | Antronix CMC5000H-A splitters, [data sheet](https://antronix.com/wp-content/uploads/2026/08/CMC5000_Splitters.pdf) | DS-1284-TM-A02, 9/2025 |
| ANX-MMC2000 | Antronix MMC2000-A MoCA splitter, [data sheet](https://antronix.com/wp-content/uploads/2026/08/MMC20000_Splitters.pdf) | DS-1197-SS-A03, 8/2020 |
| ABS-IPA3000 | Extreme Broadband IPA3004 and IPA3008, [data sheet](https://www.amphenolbroadband.com/wp-content/uploads/2021/03/EBE-PS-IPA3000DSL2-RSVFA-85-01B19-X1.pdf) | 2017 |
| ABS-180X | Amphenol Broadband ABS180xH 1.8 GHz splitters, [data sheet](https://www.amphenolbroadband.com/wp-content/uploads/2026/09/d03e3adb278ded0c448e2c1701784c8b.pdf) | DS-60-0341 2.0 |
| ABS-LPF1002 | Amphenol Broadband LPF-1002-70, [data sheet](https://www.amphenolbroadband.com/wp-content/uploads/2019/03/LPF-1002-70-Filter_DS-65-0170-Rev.-2.0.pdf) | DS-65-0170 Rev 2.0 |
| PPC-SNLP | PPC SNLP-1G MoCA point-of-entry filter, on a distributor's host, [data sheet](https://www.tvcinc.com/shared-downloads/MoCA-SNLP-1G_DataSheet.pdf) | undated |
| EBE-POE-NOTE | Extreme Broadband point-of-entry filter application note, [data sheet](https://www.amphenolbroadband.com/wp-content/uploads/2019/03/PA-POE-F201C-Series-25F09.pdf) | PA-POE-F201C 25F09 |
| TEL-4HPF | Teleste 4HPF-258, [data sheet](https://www.teleste.com/wp-content/uploads/2022/04/Teleste_Datasheet_4HPF-258_20210706.pdf) | 2021-07-06 |
| HOLLAND-CAT | Holland Electronics catalog, [data sheet](https://www.hollandelectronics.com/wp-content/uploads/2024/01/Holland-Catalog-LoRes.pdf) | undated |
