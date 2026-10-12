# Source attribution

Vendored from pollen-robotics/microduck, local microduck-upstream commit
46a380b70e70c5eb859107f4a4b529a0cb9548fc, sounds/src.
Apache-2.0; see LICENSE. The upstream Rust port credits apirrone/microduck_sounds.

Library source is unchanged. Packaging changes: standalone crate name/version,
sha2 0.10 (same SHA-256 API/output), CLI omitted. Bank version remains 5.
Observer generates the 12 greet variants, keyed by hardware_seed(), and caches
validated WAVs outside releases. It does not generate unrelated sound banks.
