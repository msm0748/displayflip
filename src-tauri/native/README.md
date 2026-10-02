# macOS display control

The native bridge lists online external displays through CoreGraphics and uses
the macOS display UUID as the saved identifier. Built-in laptop displays are
excluded because they cannot switch physical HDMI/DP inputs. Discovery does not
require a successful DDC read, so displays with disabled DDC/CI remain visible.

On Apple Silicon, the CoreDisplay dictionary supplies the exact IORegistry
framebuffer path. The bridge resolves the associated external DCPAVServiceProxy
and opens IOAVService. It never matches by model or serial alone: identical
monitors commonly report serial 0. MCDP29xx HDMI transports use address 0xB7;
other transports use 0x37. Rust owns/releases the service handles and serializes
DDC operations, validates reply framing/checksums, and retries reads.

Input selection uses MCCS VCP 0x60. The app reports unsuccessful or unverifiable
commands through its existing delivery statuses. Intel Macs can enumerate
displays, but the input-control transport currently requires Apple Silicon.
DisplayLink and some docks/adapters do not expose this DDC transport.

CoreDisplay and IOAVService are private system interfaces. Symbols are resolved
at runtime so missing symbols produce a discovery fallback or control error
instead of preventing the app from launching.

Transport discovery is adapted from MIT-licensed
[m1ddc](https://github.com/waydabber/m1ddc), revision
`04d949794102eb8df01ad3681afff6464a3eede2`. Its copyright and license are included
in `LICENSE-m1ddc.txt`; no separately installed command-line tool is required.

## Verification

Run `cargo test --manifest-path src-tauri/Cargo.toml` for protocol, identity,
unsupported transport, and existing application tests. The read-only hardware
probe is intentionally excluded from normal tests. With external DDC/CI displays
attached, run:

```sh
cargo test --manifest-path src-tauri/Cargo.toml probe_connected_monitors_read_only -- --ignored --nocapture
```

This probe only enumerates displays and reads current inputs. It does not change
inputs. Test actual switching using the app's explicit input trial buttons.
