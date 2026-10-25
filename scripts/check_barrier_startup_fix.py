#!/usr/bin/env python3
"""Apply and test the startup-gate proposal only on temporary 9091bcd source.

The reference review injects a panic before scope.spawn, modeling its
documented OS-creation-failure panic. The proposed Builder::spawn_scoped
returns io::Result::Err instead. These probes inject Err at that exact result
boundary, without exhausting OS resources. Optional startup panics check the
RAII guard's unwinding behavior separately. This is scheduler evidence, not
Bitcoin parsing, witness, UTXO, consensus, or ledger authentication.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time

PINNED_REF = "9091bcdf03b203f599d67e3c239ebbbe9bd59159"
ORIGINAL_SHA256 = "7ef5dbba94219f85f3baa5d1bf6e8f47b9ed4b9f56b3b99d8264838af6a3c66c"
PATCH_SHA256 = "b7892c22b9ea848509e56b440fec64a49c90e8bcad0a6820a626c0c11de80a3f"
PROPOSED_SHA256 = "87985bf7d84af7202936e71f953622ba738fd4c05ac63019747a6fc824f94367"
SPAWN = b"""            let spawned = thread::Builder::new().spawn_scoped(scope, move || {
                if startup.wait() {
                    work();
                }
            });"""
ERROR_ASSERT = '''    assert_eq!(
        run(&rows(140), config(4, 64), true),
        Err(ParallelIndexError::WorkerPanicked),
    );
    assert_eq!(read_only::SCAN_CALLS.load(std::sync::atomic::Ordering::Relaxed), 0);'''


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def apply_exact_patch(source, patch):
    """Strictly apply this hash-pinned single-file LF unified patch in memory."""
    if sha256(source) != ORIGINAL_SHA256 or sha256(patch) != PATCH_SHA256:
        raise SystemExit("startup proposal requires the pinned original source and exact patch")
    lines = patch.splitlines(keepends=True)
    if lines[:2] != [b"--- a/src/parallel_txindex.rs\n", b"+++ b/src/parallel_txindex.rs\n"]:
        raise SystemExit("unexpected proposed patch path")
    original = source.splitlines(keepends=True)
    output = []
    cursor = 0
    index = 2
    while index < len(lines):
        header = re.fullmatch(rb"@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@[^\n]*\n", lines[index])
        if not header:
            raise SystemExit("invalid patch hunk header")
        old_start, old_count, new_start, new_count = (
            int(header.group(1)), int(header.group(2) or 1),
            int(header.group(3)), int(header.group(4) or 1),
        )
        if old_start - 1 < cursor or old_start - 1 > len(original):
            raise SystemExit("patch hunks overlap or escape source")
        output.extend(original[cursor:old_start - 1])
        cursor = old_start - 1
        if len(output) != new_start - 1:
            raise SystemExit("proposed patch output offset mismatch")
        consumed = added = 0
        index += 1
        while index < len(lines) and not lines[index].startswith(b"@@ "):
            kind, body = lines[index][:1], lines[index][1:]
            if kind in (b" ", b"-"):
                if cursor >= len(original) or original[cursor] != body:
                    raise SystemExit("proposed patch source context mismatch")
                cursor += 1
                consumed += 1
            if kind in (b" ", b"+"):
                output.append(body)
                added += 1
            elif kind != b"-":
                raise SystemExit("unsupported proposed patch line")
            index += 1
        if (consumed, added) != (old_count, new_count):
            raise SystemExit("proposed patch hunk lengths mismatch")
    output.extend(original[cursor:])
    result = b"".join(output)
    if sha256(result) != PROPOSED_SHA256:
        raise SystemExit("patched scheduler hash mismatch")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    selection = parser.add_mutually_exclusive_group()
    selection.add_argument("--source", type=Path, help="external original candidate parallel_txindex.rs; exact pinned hash required")
    selection.add_argument("--git-ref", default=PINNED_REF, help="locally available original ref; no fetch is performed")
    parser.add_argument("--patch", type=Path, help="override fixtures/barrier_startup_cancellation.patch")
    parser.add_argument("--harness", type=Path, help="override fixtures/barrier_scheduler_fault_harness.rs.txt")
    parser.add_argument("--rustc", default=shutil.which("rustc"), help="Rust 1.90.0 compiler; initialize linker/SDK environment first")
    parser.add_argument("--timeout", type=int, default=45, help="whole normal scanner harness timeout in seconds")
    parser.add_argument("--startup-timeout", type=float, default=4, help="per injected-startup child timeout in seconds")
    parser.add_argument("--scratch-root", type=Path, help="writable temporary parent; default target/barrier-startup-review")
    parser.add_argument("--retain-artifacts", action="store_true", help="retain owned temporary sources and binaries")
    parser.add_argument("--startup-unwind-probes", action="store_true", help="also require startup panics after 0..3 starts to exit promptly")
    args = parser.parse_args()
    if not args.rustc:
        parser.error("rustc was not found; supply --rustc /path/to/pinned/rustc")
    if args.timeout <= 0 or not (0 < args.startup_timeout <= 30):
        parser.error("--timeout must be positive; --startup-timeout must be in (0, 30]")
    root = Path(__file__).resolve().parent.parent
    if args.source:
        original_raw = args.source.read_bytes()
    else:
        original_raw = subprocess.run(
            ["git", "show", f"{args.git_ref}:src/parallel_txindex.rs"], cwd=root,
            capture_output=True, check=True, timeout=30,
        ).stdout
    original = original_raw.replace(b"\r\n", b"\n")
    patch_file = args.patch or root / "fixtures/barrier_startup_cancellation.patch"
    patch_raw = patch_file.read_bytes()
    proposed = apply_exact_patch(original, patch_raw.replace(b"\r\n", b"\n"))
    if proposed.count(SPAWN) != 1:
        raise SystemExit("spawn-Err injection site must match exactly once")
    harness_file = args.harness or root / "fixtures/barrier_scheduler_fault_harness.rs.txt"
    original_harness = harness_file.read_bytes()
    harness = original_harness.decode("utf-8").replace("\r\n", "\n")
    if harness.count("__SCHEDULER_SOURCE__") != 1:
        raise SystemExit("harness source placeholder must occur exactly once")
    # Exact startup children run one test each, so this counter can also
    # prove no scanner callback entered the fixed barriers before failure.
    harness = harness.replace("mod read_only {", "mod read_only {\n    pub static SCAN_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);", 1)
    needle = "        if raw[1] != 0"
    if harness.count(needle) != 1:
        raise SystemExit("scanner counter injection site must match exactly once")
    harness = harness.replace(needle, "        SCAN_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);\n" + needle, 1)
    success_assert = "    assert!(run(&rows(140), config(4, 64), true).is_ok());"
    if harness.count(success_assert) != 1:
        raise SystemExit("startup assertion replacement must match exactly once")
    error_harness = harness.replace(success_assert, ERROR_ASSERT, 1)
    compiler = subprocess.run([args.rustc, "--version"], capture_output=True, text=True, check=True, timeout=10)
    if not compiler.stdout.startswith("rustc 1.90.0 "):
        raise SystemExit("proposal review requires pinned Rust 1.90.0")
    scratch_root = (args.scratch_root or root / "target/barrier-startup-review").resolve()
    scratch_root.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="faults-", dir=scratch_root)).resolve()
    if scratch.parent != scratch_root:
        raise SystemExit("temporary path escaped scratch root")
    print(f"pinned_ref={PINNED_REF}\noriginal_scheduler_sha256={ORIGINAL_SHA256}", flush=True)
    print(f"proposal_patch_sha256={PATCH_SHA256}\npatched_scheduler_sha256={PROPOSED_SHA256}", flush=True)
    print("mode=PROPOSED_STARTUP_GATE_PATCH_TEMP_COPY", flush=True)
    print(compiler.stdout.strip(), flush=True)
    try:
        def compile_case(name, code, template):
            candidate_file = scratch / (name + "_source.rs")
            candidate_file.write_bytes(code)
            body = template.replace("__SCHEDULER_SOURCE__", json.dumps(candidate_file.as_posix()))
            test_file = scratch / (name + ".rs")
            test_file.write_bytes(body.encode("utf-8"))
            executable = scratch / (name + (".exe" if os.name == "nt" else ""))
            subprocess.run([args.rustc, "--edition=2021", "--test", "-D", "warnings", str(test_file), "-o", str(executable)], check=True, timeout=60)
            print(f"compiled_case={name} source_sha256={sha256(code)}", flush=True)
            return executable

        def run_case(executable, exact=False):
            command = [str(executable), "--test-threads=1"]
            if exact:
                command += ["--exact", "startup_failure_case", "--nocapture"]
            before = time.monotonic()
            result = subprocess.run(command, capture_output=True, text=True, timeout=args.startup_timeout if exact else args.timeout)
            elapsed = time.monotonic() - before
            print(result.stdout + result.stderr, end="", flush=True)
            print(f"case={executable.stem} exit={result.returncode} elapsed_seconds={elapsed:.6f}", flush=True)
            return result

        normal = run_case(compile_case("normal_proposal", proposed, harness))
        if normal.returncode != 0 or "9 passed; 0 failed" not in normal.stdout:
            raise SystemExit("expected exactly nine successful normal scanner cases")
        for after in (0, 1, 2, 3):
            call = SPAWN.decode("utf-8").removeprefix("            let spawned = ").removesuffix(";")
            injection = (
                f"            let spawned = if handles.len() == {after} {{\n"
                f'                Err(std::io::Error::other("injected scoped thread creation failure after {after} workers"))\n'
                "            } else {\n                " + call + "\n            };"
            ).encode("utf-8")
            code = proposed.replace(SPAWN, injection, 1)
            result = run_case(compile_case(f"spawn_error_after_{after}", code, error_harness), exact=True)
            if result.returncode != 0 or "1 passed; 0 failed" not in result.stdout:
                raise SystemExit(f"startup Err after {after} workers did not return WorkerPanicked with zero scanner calls")
        if args.startup_unwind_probes:
            for after in (0, 1, 2, 3):
                injection = f'            if handles.len() == {after} {{ panic!("injected coordinator startup panic"); }}\n'.encode("utf-8")
                code = proposed.replace(SPAWN, injection + SPAWN, 1)
                result = run_case(compile_case(f"startup_unwind_after_{after}", code, harness), exact=True)
                if result.returncode != 101 or "1 failed" not in result.stdout:
                    raise SystemExit("startup unwind did not return the expected prompt panic exit")
        if args.source and args.source.read_bytes() != original_raw:
            raise SystemExit("external candidate source changed during review")
        if patch_file.read_bytes() != patch_raw or harness_file.read_bytes() != original_harness:
            raise SystemExit("review patch or harness changed during review")
    finally:
        if scratch.resolve().parent != scratch_root:
            raise RuntimeError("refusing cleanup outside scratch root")
        if args.retain_artifacts:
            print(f"retained_scratch={scratch}", flush=True)
        else:
            shutil.rmtree(scratch)
    print("PASS: nine normal scanner cases; four spawn-Err cancellation cases", flush=True)
    if args.startup_unwind_probes:
        print("PASS: four startup-unwind guard cases exited 101 promptly", flush=True)
    print("candidate_checkout=UNCHANGED", flush=True)
    print("BOUNDARY: synthetic scanner faults and injected spawn result; no actual OS exhaustion or consensus/parser proof", flush=True)


if __name__ == "__main__":
    main()
