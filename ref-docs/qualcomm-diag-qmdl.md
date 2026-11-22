# Qualcomm DIAG Protocol and QMDL Format

- Retrieved: 2026-11-22
- Upstream ref: EFForg/rayhunter v0.13.0 (cd462a1), local read-only checkout at `./upstream`
- Scope: the DIAG (also called QCDM / DM) protocol as used through `/dev/diag` on the Orbic RC400L
  (MDM9207), the QMDL capture format, Rayhunter's code path, the LTE log codes that matter for
  IMSI-catcher detection, and how prior-art tools handle DIAG.
- Convention: statements marked "unverified" could not be sourced from a primary document or from
  the upstream code. Sections marked "TBD (on-device)" are facts this project will measure itself.

---

## 1. What DIAG is

DIAG is Qualcomm's proprietary diagnostic protocol between a host and the modem baseband. It carries
synchronous request/response commands (version query, log configuration, NV read/write, EFS access,
subsystem commands) and asynchronous traffic from the baseband: log packets (`DIAG_LOG_F`, opcode
0x10), debug message packets (`DIAG_EXT_MSG_F` 0x79 and the terse QSR variants 0x92/0x99) and event
reports (`DIAG_EVENT_REPORT_F` 0x60). Some log packets embed raw over-the-air signalling (RRC, NAS),
which is what Rayhunter consumes. Qualcomm's own client is QXDM; its native log formats are DLF and
the later ISF.

Two transports exist:

| Transport | Where | Framing |
|---|---|---|
| USB/serial DIAG port | USB modems and phones with the DIAG interface enabled in the USB composition; appears as `/dev/ttyUSB*` via `qcserial`/`option` on Linux | HDLC-like frames only |
| `/dev/diag` character device | Inside the device's own Linux (Android or MDM "LE" Linux), provided by the `diagchar` kernel driver | 32-bit header fields plus HDLC-like frames; requires ioctls first |

Rayhunter runs on the Orbic itself and uses `/dev/diag` (root required).

## 2. Framing ("HDLC")

DIAG uses a simplified HDLC-style framing with no address/control fields:

```text
[ command code (1 byte) | payload (n bytes) | CRC-16 (2 bytes, little-endian) ] escaped, then 0x7E
```

- Terminator: `0x7E`.
- Escape: `0x7D`. `0x7E` in content becomes `0x7D 0x5E`; `0x7D` becomes `0x7D 0x5D`. The CRC bytes
  are escaped too.
- CRC: CRC-16/CCITT reflected variant (poly 0x1021, init 0xFFFF, refin/refout true, xorout 0xFFFF;
  check value 0x2189). This is the X.25 CRC.

Rayhunter implements this in `upstream/lib/src/hdlc.rs` (`hdlc_encapsulate`, `hdlc_decapsulate`) with
the CRC parameters defined as `CRC_CCITT_ALG` in `upstream/lib/src/diag/mod.rs`; the code comments
say the parameters follow QCSuper and SCAT. The unit test vector is `[01 02 03 04]` ->
`[01 02 03 04 91 39 7E]`.

QCSuper's protocol notes give the Python equivalent as
`crcmod.mkCrcFun(0x11021, initCrc=0, xorOut=0xffff)`.

## 3. Key command codes

From SCAT `src/scat/parsers/qualcomm/diagcmd.py` (cross-checked against Rayhunter where it uses them):

| Code | Name | Use |
|---|---|---|
| 0x00 | `DIAG_VERNO_F` | Version info |
| 0x10 | `DIAG_LOG_F` | Asynchronous log packet (Rayhunter parses this as `Message::Log`) |
| 0x4B | `DIAG_SUBSYS_CMD_F` | Subsystem dispatch (1-byte subsystem, 2-byte command) |
| 0x60 | `DIAG_EVENT_REPORT_F` | Event reports |
| 0x73 | `DIAG_LOG_CONFIG_F` | Log mask configuration (Rayhunter's only request type) |
| 0x79 | `DIAG_EXT_MSG_F` | Extended debug messages |
| 0x7C | `DIAG_EXT_BUILD_ID_F` | Build ID |
| 0x92 / 0x99 | `DIAG_QSR_EXT_MSG_TERSE_F` / `DIAG_QSR4_EXT_MSG_TERSE_F` | Terse (hashed) debug messages |
| 0x98 | `DIAG_MULTI_RADIO_CMD_F` | Multi-radio wrapper on newer dual-SIM firmware |

`DIAG_LOG_CONFIG_F` (0x73) operations (32-bit little-endian sub-opcode after 3 bytes of padding):

| Op | Name |
|---|---|
| 0 | `LOG_CONFIG_DISABLE_OP` |
| 1 | `LOG_CONFIG_RETRIEVE_ID_RANGES_OP` (returns 16 x u32 mask sizes, one per equipment ID) |
| 2 | `LOG_CONFIG_RETRIEVE_VALID_MASK_OP` |
| 3 | `LOG_CONFIG_SET_MASK_OP` (equipment ID, bit count, bitmask) |
| 4 | `LOG_CONFIG_GET_LOGMASK_OP` |

## 4. Log codes and masks

A 16-bit log code is `(equipment_id << 12) | item`. Equipment (subsystem) IDs relevant here:
1 = 1X/common, 4 = WCDMA, 5 = GSM, 7 = UMTS NAS, 0xB = LTE (and NR on newer basebands uses 0xB8xx).
To enable log codes, the host:

1. Sends `LOG_CONFIG_RETRIEVE_ID_RANGES_OP` to learn how many items each equipment ID supports.
2. For each equipment ID with a non-zero size, sends `LOG_CONFIG_SET_MASK_OP` with a bitmask where bit
   `i` enables code `(equip << 12) | i`.

Rayhunter does exactly this in `DiagDevice::config_logs()` and `build_log_mask_request()`
(`upstream/lib/src/diag_device.rs`, `upstream/lib/src/diag/mod.rs`). Note that a set-mask call for an
equipment ID replaces the whole mask for that ID, so any other DIAG client on the device sharing the
same mask state would be affected (unverified on this firmware; see section 9).

### 4.1 Log codes Rayhunter enables (v0.13.0)

`LOG_CODES_FOR_RAW_PACKET_LOGGING` in `upstream/lib/src/diag_device.rs`, values from
`upstream/lib/src/log_codes.rs` (which states it was copied from QCSuper):

| Code | Name | Layer |
|---|---|---|
| 0x5226 | `LOG_GPRS_MAC_SIGNALLING_MESSAGE_C` | GPRS MAC (L2) |
| 0x512F | `LOG_GSM_RR_SIGNALING_MESSAGE_C` | GSM RR (L3) |
| 0x412F | `WCDMA_SIGNALLING_MESSAGE` | WCDMA RRC (L3) |
| 0xB0C0 | `LOG_LTE_RRC_OTA_MSG_LOG_C` | LTE RRC OTA (all channels) |
| 0xB821 | `LOG_NR_RRC_OTA_MSG_LOG_C` | NR RRC (not produced by an LTE-only MDM9207; harmless) |
| 0x713A | `LOG_UMTS_NAS_OTA_MESSAGE_LOG_PACKET_C` | 2G/3G NAS |
| 0xB0E2 / 0xB0E3 | `LOG_LTE_NAS_ESM_OTA_IN/OUT_MSG_LOG_C` | LTE NAS ESM, plain |
| 0xB0EC / 0xB0ED | `LOG_LTE_NAS_EMM_OTA_IN/OUT_MSG_LOG_C` | LTE NAS EMM, plain |
| 0x11EB | `LOG_DATA_PROTOCOL_LOGGING_C` | User-plane IP (enabled, see note) |
| 0xB17F | `LOG_LTE_ML1_SERVING_CELL_MEAS_AND_EVAL_C` | RSRP/RSRQ/RSSI |
| 0xB180 | `LOG_LTE_ML1_NEIGHBOR_MEAS` | Neighbour measurements |
| 0xB062 | `LOG_LTE_MAC_RACH_RESPONSE_C` | MAC RAR (timing advance) |
| 0xB063 / 0xB064 | `LOG_LTE_MAC_DL` / `LOG_LTE_MAC_UL` | MAC transport blocks |
| 0xB114 | (unnamed, "maybe timing advance related?") | ML1 |

Note: 0x11EB enables user-plane IP logging. EFF's description of Rayhunter says it captures control
traffic but not user traffic such as web requests; how 0x11EB packets are filtered before storage was
not traced in this pass (unverified; review `upstream/lib/src/gsmtap/parser.rs` and the daemon before
relying on either statement).

### 4.2 Other LTE codes not enabled upstream (candidates)

From SCAT `diagcmd.py`:

| Code | Name | Why it might matter |
|---|---|---|
| 0xB0C1 | `LOG_LTE_RRC_MIB_MESSAGE` | MIB decode (EARFCN, PCI, bandwidth) without a full RRC parse |
| 0xB0C2 | `LOG_LTE_RRC_SERVING_CELL_INFO` | Serving cell identity, TAC, band |
| 0xB0E0 / 0xB0E1 | `LOG_LTE_NAS_ESM_SEC_OTA_IN/OUT` | Security-protected ESM (ciphered payloads; header still useful) |
| 0xB0EA / 0xB0EB | `LOG_LTE_NAS_EMM_SEC_OTA_IN/OUT` | Security-protected EMM; reveals integrity/ciphering state transitions |
| 0xB061 | `LOG_LTE_MAC_RACH_TRIGGER` | RACH cause |
| 0xB193 / 0xB197 | ML1 serving-cell measurement response / serving cell info | Cell-change and measurement tracking |

Whether the MDM9207 baseband on firmware V1.2.0 emits each of these, and at which log-packet
versions, is TBD (on-device).

### 4.3 Log packet header

`DIAG_LOG_F` frames, as parsed by Rayhunter's `Message::Log`:

| Field | Size | Notes |
|---|---|---|
| command code | u8 | 0x10 |
| pending_msgs | u8 | |
| outer_length | u16 | |
| inner_length | u16 | length of log item (log code + timestamp + body) |
| log_type | u16 | the log code (e.g. 0xB0C0) |
| timestamp | 8 bytes | Qualcomm 1.25 ms-tick based timestamp (`diaglog::Timestamp`) |
| body | inner_length - 12 | parsed by `LogBody` keyed on log code |

The 0xB0C0 body starts with an `ext_header_version` byte that selects the packet layout and the
PDU-number-to-channel mapping. Rayhunter handles several version groups
(0x02-0x08/0x0D/0x16, 0x09/0x0C, 0x0E-0x10, 0x13/0x1A/0x1B, ...) in
`upstream/lib/src/gsmtap/parser.rs`; an unknown version is an error for that packet only. The version
our baseband emits is TBD (on-device).

## 5. `/dev/diag` specifics

Sources: QCSuper "The Diag protocol" doc, Rayhunter `diag_device.rs`, Android MSM kernel
`drivers/char/diag/diagchar.h`.

1. `open("/dev/diag", O_RDWR)` (root).
2. `ioctl(fd, DIAG_IOCTL_SWITCH_LOGGING = 7, ...)` to switch to `MEMORY_DEVICE_MODE = 2` so that DIAG
   traffic is delivered to this reader instead of the USB DIAG function. Two calling conventions
   exist across kernel versions: the legacy one passes the mode directly as the third argument; newer
   kernels take a pointer to `struct diag_logging_mode_param_t { u32 req_mode; u32 peripheral_mask;
   u8 mode_param; }` plus its size. Rayhunter tries legacy first, then the struct (with
   `peripheral_mask = 0xFFFFFFFF`, `mode_param = 0`; TP-Link gets an extra
   `peripheral_mask = 0, mode_param = 1` attempt). Rayhunter's comment notes QCSuper's documented
   `int[3]` payload is wrong for some kernels because `mode_param` is only 8 bits.
3. `ioctl(fd, DIAG_IOCTL_REMOTE_DEV = 32, &int)` returns whether an extra 32-bit `device_type`
   (MDM = -1) field is present in every read/write header.
4. Writes: `struct { i32 data_type = 32 (USER_SPACE_DATA_TYPE); [i32 device_type = -1 if remote];
   u8 hdlc_frame[]; }`. Rayhunter observes `write(2)` returning 0 bytes yet the request still being
   processed, so it ignores `WriteZero`.
5. Reads: `struct { i32 data_type; [i32 device_type]; i32 num_buffers; { i32 len; u8 data[len]; }
   x num_buffers }`. Each buffer may hold several concatenated HDLC frames, which Rayhunter splits on
   `0x7E` (`MessagesContainer::messages()`). Rayhunter loops until it has read more than 8 bytes
   because some devices return short reads.

Important consequence: switching `/dev/diag` to memory-device mode takes DIAG away from the USB DIAG
function (if one is enabled in the USB composition) and from any other on-device consumer for as long
as Rayhunter holds it. Whether the Orbic's stock firmware runs its own on-device DIAG client
(for example a QXDM-mask logger or `diag_mdlog`) is TBD (on-device).

## 6. Rayhunter code path (v0.13.0)

```text
daemon/src/diag.rs  (diag task)
  DiagDevice::new(&device)                       lib/src/diag_device.rs
    -> try_new(): open /dev/diag, enable_frame_readwrite() [ioctl 7], determine_use_mdm() [ioctl 32]
    -> retried with exponential backoff (100 ms .. 5 s) for up to 30 s
  dev.config_logs()
    -> retrieve_id_ranges(): Request::LogConfig(RetrieveIdRanges)  [0x73 op 1]
    -> for each equipment ID with size > 0: set_log_mask()          [0x73 op 3]
       mask built from LOG_CODES_FOR_RAW_PACKET_LOGGING
  dev.as_stream()  -> stream of MessagesContainer (one per read(2), 10 MiB buffer)
  per container:
    QmdlWriter::write_container()   lib/src/qmdl.rs   -> raw HDLC bytes, gzip-compressed, to the
                                                         current recording in qmdl_store_path
    analysis pipeline               lib/src/analysis/*  (heuristics run over parsed messages)
  on demand (web UI / API):
    QmdlReader -> Message::from_hdlc -> gsmtap::parser::parse -> pcap.rs (pcapng with GSMTAP)
```

Analyzers present in `upstream/lib/src/analysis/`: `connection_redirect_downgrade`,
`imsi_requested`, `incomplete_sib`, `nas_null_cipher`, `null_cipher`, `priority_2g_downgrade`,
`no_nas_messages`, plus diagnostic/test analyzers.

Paths and defaults on the Orbic, from upstream `dist/` and installer code: daemon at
`/data/rayhunter/rayhunter-daemon`, config `/data/rayhunter/config.toml`, recordings under
`qmdl_store_path = "/data/rayhunter/qmdl"`, log `/data/rayhunter/rayhunter.log`, web UI on port 8080.

## 7. QMDL format

- A QMDL ("Qualcomm Mobile Diagnostic Log") file is a plain concatenation of HDLC-encapsulated DIAG
  frames, each terminated by `0x7E`, with no file header. This is how QXDM/QCAT, SCAT and MobileInsight
  treat `.qmdl`; Rayhunter's `qmdl.rs` module doc states the same.
- Rayhunter's writer gzip-compresses the stream; its reader sniffs the gzip magic (0x1F8B) and accepts
  both compressed and uncompressed input, and tolerates a truncated gzip tail (treated as EOF, not as
  an error). Recordings exported from Rayhunter therefore need `gunzip` before feeding them to tools
  that expect raw QMDL (SCAT `--qmdl`, MobileInsight offline replayer) (behaviour of those tools with
  gzip input: unverified).
- Rayhunter writes the raw read buffers it gets from `/dev/diag` (minus the `/dev/diag` header), so a
  Rayhunter QMDL contains every frame the device delivered, including responses to its own
  log-config requests, not only the enabled log codes.
- QMDL2 (newer QXDM) adds a header and hashed message tables; not relevant for the MDM9207-era
  baseband (unverified).

## 8. Prior art: how other tools handle DIAG

| Tool | Language / licence | Transport | Output | Notes |
|---|---|---|---|---|
| SCAT (fgsect) | Python, GPL-2.0 | USB DIAG interface via pyUSB (`-u -a bus:dev -i iface`), serial, or file (QMDL, Samsung SDM, HiSilicon dumps) | GSMTAP over UDP (127.0.0.1:4729 control plane, :47290 user plane), optional pcap | Qualcomm (`-t qc`), Samsung (`-t sec`), HiSilicon (experimental). GSMTAPv3 + Lua dissector for NR and LTE MAC/PDCP. No on-device agent; explicitly will not ship an Android companion. |
| QCSuper (P1sec) | Python, GPL-3.0 | `--adb` (pushes a helper that bridges `/dev/diag` over ADB/TCP on rooted Android), `--usb-modem` (serial DIAG), DLF/QMDL file input | pcap (GSMTAP), live Wireshark, DLF dump/read | Source of Rayhunter's log-code table and the `/dev/diag` ioctl notes. Its "The Diag protocol.md" is the best public description of `/dev/diag` framing. |
| MobileInsight (UCLA WiNG / Purdue) | Python + C++ (`mobileinsight-core`); README says Apache 2.0 | On-device Android app reading `/dev/diag` on rooted phones; desktop "online monitor" on a serial DIAG port; offline replayer for QMDL/mi2log | Decoded XML/JSON-like message objects, analyzers for RRC/EMM/ESM | Large catalogue of Qualcomm log-packet decoders with per-version layouts; useful as a cross-reference for 0xB0C0/0xB17F/0xB193 layouts. |
| SnoopSnitch (SRLabs) | Android app, GPL-3.0 | Reads Qualcomm `/dev/diag` on rooted Android phones via a native helper (inference from SRLabs documentation; unverified in source this pass) | On-device analysis (IMSI-catcher and SS7 heuristics), optional upload to GSMmap | Upstream last pushed 2022-05-17; heuristics centred on 2G/3G. |
| Rayhunter (EFF) | Rust, GPL-3.0 | On-device `/dev/diag` (memory-device mode) on MDM9x07-class hotspots | gzip QMDL + per-recording analysis; pcapng with GSMTAP on export | See section 6. |

## 9. TBD (on-device)

All of the following are to be measured on our unit (ORB400L_V1.2.0_BVZRT) with Rayhunter stopped
unless stated, and recorded here with the exact command used:

- `ls -l /dev/diag`, owner/group/mode, and which processes hold it open (`ls -l /proc/*/fd | grep diag`).
- Kernel diag driver behaviour: which `DIAG_IOCTL_SWITCH_LOGGING` convention succeeds (legacy vs
  struct), and the value returned by `DIAG_IOCTL_REMOTE_DEV`.
- `LOG_CONFIG_RETRIEVE_ID_RANGES_OP` result (the 16 mask sizes) for this baseband.
- `ext_header_version` values observed for 0xB0C0, and versions for 0xB17F/0xB180/0xB062/0xB063/0xB064.
- Whether 0xB0C1, 0xB0C2, 0xB0E0/E1/EA/EB are emitted.
- Baseband build string (`DIAG_EXT_BUILD_ID_F`) for cross-reference with firmware version.
- Whether the USB composition exposes a DIAG interface in any mode (PID 0xf601 or others), and
  whether host-side SCAT works against it while Rayhunter is stopped.
- Throughput: bytes/s of QMDL written under idle and active LTE; storage growth per day.

## Sources

- <https://github.com/EFForg/rayhunter> (v0.13.0; files cited: `lib/src/diag_device.rs`, `lib/src/diag/mod.rs`, `lib/src/hdlc.rs`, `lib/src/log_codes.rs`, `lib/src/qmdl.rs`, `lib/src/gsmtap/parser.rs`, `lib/src/diag/diaglog/mod.rs`, `daemon/src/diag.rs`)
- <https://github.com/P1sec/QCSuper/blob/master/docs/The%20Diag%20protocol.md>
- <https://github.com/P1sec/QCSuper>
- <https://github.com/fgsect/scat>
- <https://github.com/fgsect/scat/blob/master/src/scat/parsers/qualcomm/diagcmd.py>
- <https://github.com/mobile-insight/mobileinsight-core>
- <http://mobileinsight.net>
- <https://github.com/srlabs/snoopsnitch>
- <https://opensource.srlabs.de/projects/snoopsnitch>
- <https://android.googlesource.com/kernel/msm.git/+/android-7.1.0_r0.3/drivers/char/diag/diagchar.h>
- <https://www.eff.org/deeplinks/2025/03/meet-rayhunter-new-open-source-tool-eff-detect-cellular-spying>
