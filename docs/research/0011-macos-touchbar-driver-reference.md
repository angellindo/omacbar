# 0011 · macOS Touch Bar driver reference: what the local sample proves

Status: resolved within static-evidence limits (TB-41) · Inspected 2026-11-22 UTC.

## Question

What does the newly requested local macOS driver collection establish about Touch Bar matching,
display ownership and input, and what remains unknown when comparing macOS, the T2 plugin and Rune?

## Findings

### Evidence set and reproduction boundary

The requested collection's `drivers/README.md` describes samples from macOS **13.7.8** and marks
Apple binaries as private interoperability references. This is a different collection from the
macOS 13.7 material described in [research 0006](0006-macos-reference-stack.md). Its
`samples/sensors/com.apple.driver.usb.AppleUSBiBridge/SOURCE.txt` identifies the original bundle as
`/System/Library/Extensions/AppleUSBiBridge.kext` and extraction from
`/System/Library/KernelCollections/BootKernelExtensions.kc`. The source locator, rather than an
assumed equivalence with older files, governs the observations below. Confidence: **high for
metadata observed; extraction provenance is supplied by the collection, not independently certified**.

The relevant sample consists of `binary` (35,856 bytes), `bundle/Contents/Info.plist`, and
`SOURCE.txt`. Inventory under the requested collection found no separate AppleEmbeddedOSSupportHost,
DFRDisplay, DFRTouchServiceFilter, TouchBarServer or ControlStrip artifact. Their absence from
this set does not mean macOS lacks them. Runtime ioreg/nvram reports and unrelated private notes
were not used. Confidence: **high for this bounded inventory**.

To reproduce, select that bundle identifier in the local collection, decode its Info.plist with
Python `plistlib`, and inspect only the personality/field locators in the table below. For binary
names, scan printable ASCII strings or parse the Mach-O symbol records without running the file.
No Apple code, firmware, raw plist, data table, extracted binary, private absolute path or machine
identifier is included here. The following SHA-256 values identify source files, not hardware:

| Artifact | SHA-256 | Confidence |
|---|---|---|
| `bundle/Contents/Info.plist` | `74dd6f369934c24a2d363a7e4d7781d89e45442b63104e899500e107ce6598eb` | high, computed read-only |
| `binary` | `647cc0b37c703f1826eccc5791fdf66a8a06c0c54de2a673beb0230e289514a9` | high, computed read-only |

These locators are reproducible by someone with the authorized local artifacts. The proprietary
sources are deliberately not linked as public downloads; this limits independent public review.

### IOKit matching and USB configuration

Every row below is a direct observation from the identified plist, at
`IOKitPersonalities → <personality> → <field>`. Numeric USB ids are generic product identifiers.
All confidence ratings are **high for declared matching/property values**, not proof of runtime state.

| Personality / fields | Observation | Consequence |
|---|---|---|
| `iBridge1,1 Driver`: `IOClass`, `IOProviderClass`, `idVendor`, `idProduct`, `bcdDevice` | AppleUSBiBridge binds IOUSBHostDevice for vendor 0x05ac, product 0x8600, revision 0x0101 | This is a T1 device match, not a panel renderer declaration |
| Same: `kUSBPreferredConfiguration`, `kUSBPreferredRecoveryConfiguration` | Preferred 2; recovery 3 | Corroborates the T1 config-2 path in research 0001; does not show the machine's current configuration |
| `iBridge1,1 Display IOUSBInterface`: `bConfigurationValue`, `bInterfaceNumber`, `IOProviderMergeProperties/UsbUserClientEntitlementRequired` | Configuration 2, interface 3; `com.apple.appledfr.client` entitlement | The display interface is distinct from the device configuration selector and access is gated |
| `iBridge2,1 Display IOUSBHostDevice`: product/revision and merged preferred configuration properties | Product 0x8302, revision 0x0201; preferred 2, recovery 1 | The same plist has a different display-device personality; recovery behavior is not interchangeable with T1 |
| `iBridge2,1 Display IOUSBInterface`: configuration/interface/merged entitlement | Configuration 2, interface 1; same DFR client entitlement | Interface numbers differ across these personalities despite both using config 2 |
| `iBridge1,1 Digitizer IOUSBHostHIDDevice`: `DeviceUsagePairs`, merged `kUSBHIDDeviceIdlePolicy` | Vendor usage page 0xFF00, usage 0x16; idle policy 1000 | Identifies HID matching and an idle policy, not a decoded touch report format or screen-off timer |
| `iBridge1,1 ALS IOUSBHostHIDDevice`: same fields | Sensor page 0x20, usage 0x41; idle policy 150 | Ambient-light HID is separate from display pixels; the value is not an OLED brightness scale |

The display personalities use **AppleUSBHostMergeProperties**, not a class claiming to implement
pixel upload. Their fields describe matching and properties merged into the provider. A client
entitlement string does not identify the sole process holding the display, the USB endpoint numbers,
a framebuffer format or an upload protocol. Confidence: **high for the class/property distinction;
those downstream details remain unverified in this set**.

### Binary evidence and limitations

The sample is an x86-64 Mach-O with AppleUSBiBridge class/method symbol names. Minimal observed
locators include `AppleUSBiBridge::start`, `powerStateWillChangeTo`, `powerStateDidChangeTo`,
`initialPowerStateForDomainState`, and strings for preferred configuration and configuration
failure. These are consistent with device startup/configuration and power-management participation.
They do **not** establish the order of USB requests or the power-state implementation.
Source: `binary`, printable symbol/string names. Confidence: **high for name presence; medium for
architectural inference**.

`llvm-objdump --macho --disassemble --demangle` rejected this sample as malformed/truncated:
`LC_SYMTAB`'s string-table range exceeds the file. Reading the Mach-O load command confirms the
range inconsistency. This research did not repair, execute or decompile the sample. Therefore no
claim here rests on a verified disassembly or call graph. In particular, a power method name is
not evidence of a panel brightness command, idle-dimming duration or suspend/resume reliability.
Confidence: **high for inspection-tool failure and header inconsistency; behavior unverified**.

### Reassessing the older userspace reference

[Research 0006](0006-macos-reference-stack.md) records symbol/string observations from a different
collection. The new sample neither independently reproduces nor disproves those observations.
They should be used with these narrower interpretations:

| Earlier observation | Supported interpretation / limit | Confidence |
|---|---|---|
| `DFRDisplayDrawRectsWithIOSurfaceAsync`, sparse-region/tile conversion names | Evidence of a surface/rectangle-oriented API vocabulary in that older userspace sample. Does not prove that only changed rectangles cross USB, that IOSurface bytes are wire bytes, or a particular tile size/encoding. The older statement “partial updates, not full frames” is too strong as a transport conclusion. | medium for API inference from recorded names; wire behavior unverified |
| Brightness/dimming method names | Suggest separate brightness/idle policy in the userspace stack; exact command format and state transitions are not reconstructed. | medium inference; parameters/timing unverified |
| `DefaultFNMode`, presentation modes, Escape component | Suggest policy for what Fn presents and separate Escape UI. Do not prove six Fn/modifier layers, a universal fixed Escape pixel reservation, or Linux event semantics. | medium inference |
| DFRTouchServiceFilter usage matching | Suggests an HID filter stage in the older sample; does not decode digitizer reports or identify all transformations before application input. | medium inference; pipeline details unverified |

For actual Linux transfer behavior there is stronger public code evidence: pinned `appletbdrm`
walks DRM damage, converts XRGB8888 to BGR888, and issues bulk USB transfers; pinned tiny-dfr
renders into a dumb framebuffer and marks clips dirty. See
[research 0010](0010-ewweberlin-touchbar-control-path.md). That proves the referenced Linux path,
not Apple's implementation and not the T1 patch's equivalence.

## Decision input

- Retain the separation **presentation policy → render surface → display backend → kernel/USB**.
  It is supported by our [ADR-0002](../adr/0002-display-backend-is-a-seam.md) and public Linux code;
  proprietary symbol names are corroborating architecture hints, not an implementation spec.
- Treat damage rectangles as a backend capability to evaluate, not a macOS-derived requirement
  that all transports must support. The old 0006 recommendation to use only damaged rectangles
  needs that qualification. Correct orientation, format, ownership and a whole-frame baseline
  still need to be established on the selected T1 backend.
- Do not equate the T2 plugin's working report with T1 support. Both macOS personalities select
  config 2 but use different display interfaces; Linux v6.18's cited stock driver matches the T2
  product, while our T1 path depends on research 0001's patches. No current hardware state was read.
- Proposed follow-up, only if needed for an unresolved protocol question: inspect an authorized,
  complete matching userspace/framework sample or public driver implementation, recording exact
  call/data evidence. This blocks claims about Apple's wire protocol, not Rune simulator work or
  this research's completion. Do not copy proprietary assets or translate Apple code into the repo.

Confidence: **high** for the source boundaries and compatibility limitation; backend recommendations
are **design inferences**, pending the existing human hardware gates.

## Sources

- Local collection `drivers/README.md`; sample's `SOURCE.txt`, plist personality fields and binary
  symbol/header observations, identified above. Read-only static analysis; no Apple code executed.
- [0001 · T1 hardware](0001-t1-touch-bar-hardware.md),
  [0006 · prior macOS reference](0006-macos-reference-stack.md),
  [0010 · plugin/public transport trace](0010-ewweberlin-touchbar-control-path.md).
- Project base reviewed: `7bbc4dd0e62c46916dc9056ba0f744a6ebcd1d34`.

Resumen: la muestra nueva confirma **selección USB y separación de interfaces**, no contiene el
compositor completo. Los nombres de IOSurface/rectángulos del análisis anterior no demuestran el
protocolo USB de Apple. Sirven para orientar la separación de responsabilidades; para implementar
Rune debemos usar evidencia pública del backend Linux y validar T1 por separado.
