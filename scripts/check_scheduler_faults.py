#!/usr/bin/env python3
"""Fault-inject an exact pinned scheduler, using only Python/Rust stdlib.

The default includes unmodified source. An opt-in proposed panic-parity patch
applies only to a temporary copy after verifying the original source hash.
The read_only dependency is replaced with deterministic delays, scan errors
and panics. This is scheduler evidence, not Bitcoin or consensus validation.
"""

import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

PINNED_REF = "d4ba7592de3a93ee59ded6ebb2cd0cec91174368"
PINNED_SHA256 = "9c27880fb09a92467f69c254bad1735fb8ffd5e295aeddcc86b6309938b5c4a5"
PANIC_PARITY_BEFORE = b"""                        let failed = row.is_err();
                        local.push(row);
                        if failed {
                            break;
                        }
"""
PANIC_PARITY_AFTER = b"""                        local.push(row);
"""


def apply_panic_parity_fix(source, root):
    if hashlib.sha256(source).hexdigest() != PINNED_SHA256:
        raise SystemExit("proposed panic-parity patch requires the original pinned candidate")
    if source.count(PANIC_PARITY_BEFORE) != 1:
        raise SystemExit("proposed panic-parity patch must match exactly one source location")
    patched = source.replace(PANIC_PARITY_BEFORE, PANIC_PARITY_AFTER, 1)
    proposed_diff = "".join(difflib.unified_diff(
        source.decode("utf-8").splitlines(keepends=True),
        patched.decode("utf-8").splitlines(keepends=True),
        fromfile="a/src/parallel_txindex.rs", tofile="b/src/parallel_txindex.rs",
    ))
    patch_file = root / "fixtures/pooled_panic_parity.patch"
    if patch_file.read_text(encoding="utf-8") != proposed_diff:
        raise SystemExit("proposed panic-parity diff does not match the exact temporary-source change")
    return patched


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--source", type=Path, help="candidate parallel_txindex.rs")
    source.add_argument("--git-ref", default=PINNED_REF)
    parser.add_argument("--rustc", default=shutil.which("rustc"), help="pinned Rust compiler path")
    parser.add_argument("--expected-sha256", default=PINNED_SHA256)
    parser.add_argument("--apply-panic-parity-fix", action="store_true",
                        help="test the proposed patch on a temporary copy of the pinned candidate only")
    parser.add_argument("--timeout", type=int, default=45, help="whole harness timeout in seconds")
    parser.add_argument("--scratch-root", type=Path, help="writable temporary parent (default: target/scheduler-review)")
    args = parser.parse_args()
    if not args.rustc:
        parser.error("rustc was not found; supply --rustc /path/to/pinned/rustc")
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    root = Path(__file__).resolve().parent.parent
    if args.source:
        raw_source = args.source.read_bytes()
    else:
        raw_source = subprocess.run(
            ["git", "show", f"{args.git_ref}:src/parallel_txindex.rs"], cwd=root,
            check=True, capture_output=True, timeout=30,
        ).stdout
    normalized_source = raw_source.replace(b"\r\n", b"\n")
    digest = hashlib.sha256(normalized_source).hexdigest()
    if digest != args.expected_sha256:
        raise SystemExit(f"scheduler SHA-256 mismatch: expected {args.expected_sha256}, got {digest}")
    compiled_source = (apply_panic_parity_fix(normalized_source, root)
                       if args.apply_panic_parity_fix else normalized_source)
    compiled_digest = hashlib.sha256(compiled_source).hexdigest()
    mode = "PROPOSED_PANIC_PARITY_PATCH_TEMP_COPY" if args.apply_panic_parity_fix else "UNMODIFIED_CANDIDATE"
    harness = (root / "fixtures/txindex_scheduler_fault_harness.rs.txt").read_text(encoding="utf-8")
    scratch_root = (args.scratch_root or root / "target/scheduler-review").resolve()
    scratch_root.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="faults-", dir=scratch_root)).resolve()
    if scratch.parent != scratch_root:
        raise SystemExit("temporary path escaped scratch root")
    try:
        source_path = scratch / "parallel_txindex.rs"
        source_path.write_bytes(compiled_source)
        harness = harness.replace("__SCHEDULER_SOURCE__", json.dumps(source_path.as_posix()))
        harness_path = scratch / "scheduler_fault_harness.rs"
        harness_path.write_text(harness, encoding="utf-8")
        binary = scratch / ("scheduler_fault_harness.exe" if os.name == "nt" else "scheduler_fault_harness")
        compiler_args = [args.rustc, "--edition=2021", "--test"]
        if args.apply_panic_parity_fix:
            compiler_args.extend(["--cfg", "panic_parity_fixed"])
        compiler_args.extend([str(harness_path), "-o", str(binary)])
        subprocess.run(compiler_args, check=True, timeout=60)
        # A deadlock is a failed check, rather than an indefinitely hung job.
        subprocess.run([str(binary), "--test-threads=1"], check=True, timeout=args.timeout)
    finally:
        # Validate the final absolute target immediately before recursive
        # cleanup, staying in Python for the entire filesystem operation.
        if scratch.resolve().parent != scratch_root:
            raise RuntimeError("refusing cleanup outside scratch root")
        shutil.rmtree(scratch)
    print(f"PASS: mode={mode}")
    print(f"original_scheduler_sha256={digest}")
    print(f"compiled_scheduler_sha256={compiled_digest}")
    print("candidate_checkout=UNCHANGED")
    print("BOUNDARY: synthetic prepared-scanner faults; no parser, witness, UTXO or consensus authentication")


if __name__ == "__main__":
    main()
