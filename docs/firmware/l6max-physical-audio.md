# L6max physical audio rig observations

These observations establish an initial host-side discovery only. The
connected device's firmware identity and mixer settings were not read, and
the measured paths were not isolated from possible interface monitoring.
They do not qualify modified firmware or establish physical channel polarity.

On 2026-10-30, the operator reported AudioFuse 16Rig outputs connected to
the L6max stereo channel 5 and the L6max master outputs returned to the
interface, using the same connections as the Multistomp rig. Individual
left/right cable orientation was unknown. The Multistomp harness used
playback AUX6 and AUX7, and capture AUX2 and AUX3 (zero-based JACK names).
Its `audio.py` SHA-256 was
`6f7f6b230db83cb42d6920977765d29f371e3646f0be12f1f6cc633d0767686e`.

Without sending MIDI or changing mixer settings, that harness sent separate
1.5-second pseudorandom bursts at -25 dBFS and recorded at 48 kHz, block size
128. It reported send 0 to return 0 and send 1 to return 1, each with
correlation about 0.60 and fitted gain about -38 dB. No xruns, nonfinite
samples or clipping were reported; the recorded peaks were 0.00246 and
0.00265 full scale. Timing included the host and interface, and the matched
sample lag is not a standalone device latency measurement. Two swapped cable
pairs could also produce the observed end-to-end mapping. The idle segment
showed energy near 60 Hz and harmonics; the source is unknown.

The send WAV, return WAV and report SHA-256 values were
`8bc887854db91d5b2c492a40e77ecc84722ae140d7b6d1f6211d4873db149e17`,
`2c04ba8bf855aa6d7d34addaa138a10096cbb08412dd9e5f0e9bf8c719ba4582`, and
`cb24a871c3ff8bb273824084b7225c9b1985d1e60c5234b14c0faa31cc9a9fd5`.
The recordings remain local and are not committed. A controlled isolation
check, firmware identification and known mixer state are still needed before
using this rig to compare hardware with the simulator or DSP implementation.

The same physical connection enumerated as ZOOM `1686:08f5` at 12 Mbit/s,
with a nine-byte configuration declaring zero interfaces. Linux consequently
exposed no L6max audio or MIDI ports. For comparison, a disposable QEMU build
changed the modeled `PORTSC` speed bits from high speed to full speed on
connect and controller reset. The `l6test` headless `usb` case still passed
and observed five interfaces and a 507-byte configuration. That experiment
only changes the modeled register bits; it does not model physical USB
negotiation or explain the physical connection's state. The device firmware
identity remains unknown.

The modified QEMU USB source, executable and scenario-result SHA-256 values
were `289d77cd9df0e3ca717425bda93a95a82ec2333030636d0dcf6d4b6dc2ba69d6`,
`ec3e724925cb39ddf78af06f1ced09433af323c15e338aee6390a05b87f7df25`, and
`2e2581a21935d012fbcd011ce7d11c36de3cc6fe1d6f15f14fd2f4df61558936`.
The emulator's build source provenance was not independently attested. The
stock firmware package used in QEMU had SHA-256
`4e20f92c9c5131ed082c0feec2b2d7d0445fb576296f11f6dace56a4fc0c9b47`.
