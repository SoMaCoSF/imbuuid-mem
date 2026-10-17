// Shared build-time provenance minter (SOM-DOC-10435).
// The shared file's `fn main` is the build-script entry point, so it is
// included at crate root. (Fixed 2026-10-17: the shared file's original `//!`
// header broke the documented include! usage with E0753 for every consumer;
// converted to `//`, backup at build-support/provenance-build.rs.bk-20261017.)

include!("../../build-support/provenance-build.rs");
