// SOM-DOC-10435 (part) — build-time provenance mint hook.
// Mints a GYST UUIDv8 type 0x403 committing to (source-tree SHA-256 ||
// timestamp || builder) and exposes IMBUUID_PROVENANCE_UUID etc. via
// cargo:rustc-env. Shared implementation: never fork this file.
include!("../../build-support/provenance-build.rs");
