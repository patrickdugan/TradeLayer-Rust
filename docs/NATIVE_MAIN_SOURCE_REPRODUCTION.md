# Native Main source reproduction

Run from the repository root with the existing genuine NeDB1.8.0 module prefix
in TL_JS_MODULE_ROOT or TRADELAYER_JS_ROOT. No installation/network is needed.

    node scripts/native_main_controls.cjs --check
    node scripts/native_main_controls.cjs --check --lock
    node scripts/native_processed_retry_prefix.cjs --check
    node scripts/native_processed_retry_prefix.cjs --check --lock

The scripts default to their repository root, use the repository-local
fixtures/reference_fidelity/native_main_source_inputs.json and existing pinned
scripts/main_service_vectors.cjs. Original archived module/fixture dependencies
are checked locally. PROOF_REPO may explicitly select a different input checkout;
no sibling outputs/work directory is assumed. This addendum changes no Rust or
generated output fixture. Default/locked controls preserve ce12d756/369103e5.

Checks and source hashes normalize CRLF to LF for checkout portability. These
hashes identify reviewed reproduction inputs; they do not authenticate L1 data
or authorize financial state.
