#!/usr/bin/env python3
"""Review exact 9091bcd barrier scheduling with bounded synthetic faults.

Nine tests include the unmodified candidate and replace only read_only with
deterministic scanner delays/errors/panics. Four separate temporary-source
probes inject a panic before the first/second/third/fourth scoped spawn. The
panic models scope.spawn's documented OS-creation-failure panic, without
exhausting actual OS resources. A timeout after a partial startup is the
expected characterization of this pinned defect, NOT successful recovery.
No parser, hashing, witness, UTXO, or consensus correctness is exercised.
"""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

PINNED_REF = "9091bcdf03b203f599d67e3c239ebbbe9bd59159"
PINNED_SHA256 = "7ef5dbba94219f85f3baa5d1bf6e8f47b9ed4b9f56b3b99d8264838af6a3c66c"
SPAWN_SITE = b"            handles.push(scope.spawn(move || loop {\n                start.wait();"


def emit_process_output(result):
    output = result.stdout or ""
    if isinstance(output, bytes):
        output = output.decode("utf-8", errors="replace")
    print(output, end="" if output.endswith("\n") else "\n", flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    source = parser.add_mutually_exclusive_group()
    source.add_argument("--source", type=Path, help="external candidate parallel_txindex.rs; exact pinned hash required")
    source.add_argument("--git-ref", default=PINNED_REF, help="locally available candidate ref; no fetch is performed")
    parser.add_argument("--rustc", default=shutil.which("rustc"), help="Rust 1.90.0 compiler path; initialize linker/SDK environment first")
    parser.add_argument("--harness", type=Path, help="override fixtures/barrier_scheduler_fault_harness.rs.txt")
    parser.add_argument("--timeout", type=int, default=45, help="whole unmodified harness timeout in seconds")
    parser.add_argument("--startup-timeout", type=float, default=4, help="per injected-startup probe timeout in seconds")
    parser.add_argument("--scratch-root", type=Path, help="writable temporary parent (default target/barrier-review)")
    parser.add_argument("--retain-artifacts", action="store_true", help="retain owned temporary sources/binaries for inspection")
    args = parser.parse_args()
    if not args.rustc:
        parser.error("rustc was not found; supply --rustc /path/to/pinned/rustc")
    if args.timeout <= 0 or not (0 < args.startup_timeout <= 30):
        parser.error("--timeout must be positive; --startup-timeout must be in (0, 30]")
    root = Path(__file__).resolve().parent.parent
    if args.source:
        raw_source = args.source.read_bytes()
    else:
        raw_source = subprocess.run(
            ["git", "show", f"{args.git_ref}:src/parallel_txindex.rs"], cwd=root,
            check=True, capture_output=True, timeout=30,
        ).stdout
    normalized = raw_source.replace(b"\r\n", b"\n")
    digest = hashlib.sha256(normalized).hexdigest()
    if digest != PINNED_SHA256:
        raise SystemExit(f"scheduler SHA-256 mismatch: expected {PINNED_SHA256}, got {digest}")
    if normalized.count(SPAWN_SITE) != 1:
        raise SystemExit("startup-injection site must match exactly once")
    harness_path = args.harness or root / "fixtures/barrier_scheduler_fault_harness.rs.txt"
    harness_template = harness_path.read_text(encoding="utf-8")
    if harness_template.count("__SCHEDULER_SOURCE__") != 1:
        raise SystemExit("harness source placeholder must occur exactly once")
    scratch_root = (args.scratch_root or root / "target/barrier-review").resolve()
    scratch_root.mkdir(parents=True, exist_ok=True)
    scratch = Path(tempfile.mkdtemp(prefix="faults-", dir=scratch_root)).resolve()
    if scratch.parent != scratch_root:
        raise SystemExit("temporary path escaped scratch root")
    print(f"pinned_ref={PINNED_REF}\noriginal_scheduler_sha256={digest}", flush=True)
    print("mode=UNMODIFIED_BARRIER_PLUS_TEMPORARY_STARTUP_FAILURE_CHARACTERIZATION", flush=True)
    try:
        def compile_case(name, code):
            candidate = scratch / (name + "_source.rs")
            candidate.write_bytes(code)
            harness = harness_template.replace("__SCHEDULER_SOURCE__", json.dumps(candidate.as_posix()))
            harness_file = scratch / (name + ".rs")
            harness_file.write_text(harness, encoding="utf-8")
            binary = scratch / (name + (".exe" if os.name == "nt" else ""))
            subprocess.run([args.rustc, "--edition=2021", "--test", str(harness_file), "-o", str(binary)], check=True, timeout=60)
            print(f"compiled_case={name} sha256={hashlib.sha256(code).hexdigest()}", flush=True)
            return binary

        normal = compile_case("unmodified", normalized)
        result = subprocess.run([str(normal), "--test-threads=1"], check=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=args.timeout)
        emit_process_output(result)
        if "9 passed; 0 failed" not in result.stdout:
            raise SystemExit("expected exactly nine unmodified candidate tests")
        for after in (0, 1, 2, 3):
            panic = (f'            if handles.len() == {after} {{ panic!("injected scoped thread creation failure after {after} workers"); }}\n').encode()
            injected = normalized.replace(SPAWN_SITE, panic + SPAWN_SITE, 1)
            binary = compile_case("startup_failure_after_" + str(after), injected)
            try:
                result = subprocess.run([str(binary), "--exact", "startup_failure_case", "--test-threads=1", "--nocapture"], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, timeout=args.startup_timeout)
                emit_process_output(result)
            except subprocess.TimeoutExpired as error:
                # subprocess.run kills and waits for only this exact owned
                # direct child before propagating TimeoutExpired.
                emit_process_output(error)
                print(f"startup_after={after} outcome=TIMEOUT_{args.startup_timeout:g}s_owned_child_killed_and_waited", flush=True)
                if after == 0:
                    raise SystemExit("unexpected timeout with zero workers started")
            else:
                print(f"startup_after={after} outcome=EXIT_{result.returncode}", flush=True)
                if after != 0 or result.returncode != 101:
                    raise SystemExit("startup behavior differs from the pinned defect characterization")
        if args.source and args.source.read_bytes() != raw_source:
            raise SystemExit("external candidate source changed during review")
    finally:
        if scratch.resolve().parent != scratch_root:
            raise RuntimeError("refusing cleanup outside scratch root")
        if args.retain_artifacts:
            print(f"retained_scratch={scratch}", flush=True)
        else:
            shutil.rmtree(scratch)
    print("PASS: nine scanner-fault cases and four startup-failure characterizations", flush=True)
    print("DEFECT: partial scoped-worker startup hangs the fixed-size barrier team", flush=True)
    print("candidate_checkout=UNCHANGED", flush=True)
    print("BOUNDARY: synthetic scanner/startup faults; no actual OS exhaustion or parser/consensus authentication", flush=True)


if __name__ == "__main__":
    main()
