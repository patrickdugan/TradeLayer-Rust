#!/usr/bin/env python3
"""Offline same-host CPU / peak-RSS comparison on local Bitcoin Core raw hex blocks.

No RPC, wallets or remote hosts are contacted. Each variant starts in a fresh
process so VmHWM measures peak RSS independently, including shared parse costs.
"""
import argparse
import json
import statistics
import subprocess
import sys
from pathlib import Path


def windows_peak_working_set_kib(process):
    """Peak working set of an exited child, read through its still-open handle.

    The Windows counterpart of VmHWM: the child's own high-water mark, taken
    by the parent after exit, so the child needs no unsafe code.
    """
    import ctypes
    from ctypes import wintypes

    class Counters(ctypes.Structure):
        _fields_ = [
            ("cb", wintypes.DWORD),
            ("PageFaultCount", wintypes.DWORD),
            ("PeakWorkingSetSize", ctypes.c_size_t),
            ("WorkingSetSize", ctypes.c_size_t),
            ("QuotaPeakPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPagedPoolUsage", ctypes.c_size_t),
            ("QuotaPeakNonPagedPoolUsage", ctypes.c_size_t),
            ("QuotaNonPagedPoolUsage", ctypes.c_size_t),
            ("PagefileUsage", ctypes.c_size_t),
            ("PeakPagefileUsage", ctypes.c_size_t),
        ]

    counters = Counters()
    counters.cb = ctypes.sizeof(Counters)
    query = ctypes.WinDLL("kernel32", use_last_error=True).K32GetProcessMemoryInfo
    query.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
    query.restype = wintypes.BOOL
    if not query(wintypes.HANDLE(int(process._handle)), ctypes.byref(counters), counters.cb):
        raise OSError(ctypes.get_last_error(), "K32GetProcessMemoryInfo failed")
    return counters.PeakWorkingSetSize // 1024


def bench(binary, block, workers, repeats, variant):
    cmd = [str(binary), str(block), str(workers), str(repeats), variant]
    with subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) as process:
        stdout, stderr = process.communicate()
        if process.returncode:
            raise subprocess.CalledProcessError(process.returncode, cmd, stdout, stderr)
        working_set = windows_peak_working_set_kib(process) if sys.platform == "win32" else None
    output = dict(line.split("=", 1) for line in stdout.splitlines() if "=" in line)
    if working_set is not None:
        output["process_peak_working_set_kib"] = str(working_set)
    required = {
        "variant",
        "block_hash",
        "transaction_count",
        "raw_block_bytes",
        "prepared_only_median_ms",
        "process_peak_rss_kib",
        "txid_merkle",
        "full_bitcoin_consensus",
    }
    missing = required - output.keys()
    if missing:
        raise ValueError(f"missing metrics from {variant}: {sorted(missing)}")
    if output["variant"] != variant or output["txid_merkle"] != "LOCAL_CHECK_ONLY":
        raise ValueError(f"invalid output from {variant}")
    if output["full_bitcoin_consensus"] != "UNVERIFIED":
        raise ValueError("unexpected authority field")
    return output


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("blocks", nargs="+", type=Path, help="local getblock(hash,0) hex files")
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--trials", type=int, default=4)
    parser.add_argument(
        "--binary",
        type=Path,
        default=Path("target/release/tradelayer-real-block-reduce-bench"),
    )
    args = parser.parse_args()
    if not (1 <= args.workers <= 16 and 1 <= args.repeats <= 20 and 1 <= args.trials <= 20):
        parser.error("workers 1..16, repeats 1..20 and trials 1..20 required")
    if not args.binary.is_file():
        parser.error(f"build the release binary first: {args.binary}")

    for block in args.blocks:
        if not block.is_file():
            parser.error(f"missing local block: {block}")
        measurements = {"tree": [], "hash": []}
        identities = set()
        for trial in range(args.trials):
            order = ("tree", "hash") if trial % 2 == 0 else ("hash", "tree")
            for variant in order:
                observed = bench(args.binary, block, args.workers, args.repeats, variant)
                identities.add(
                    (
                        observed["block_hash"],
                        observed["transaction_count"],
                        observed["raw_block_bytes"],
                    )
                )
                measurements[variant].append(observed)
        if len(identities) != 1:
            raise ValueError("source file changed or parity metadata differs during trials")
        block_hash, count, byte_size = next(iter(identities))
        def median_metric(variant, key):
            values = [row[key] for row in measurements[variant] if key in row]
            if key == "process_peak_rss_kib" and "UNAVAILABLE" in values:
                return None
            if key == "process_peak_working_set_kib" and len(values) != len(measurements[variant]):
                return None
            return statistics.median(float(value) for value in values)

        tree_ms = median_metric("tree", "prepared_only_median_ms")
        hash_ms = median_metric("hash", "prepared_only_median_ms")
        record = {
            "source_file": str(block),
            "block_hash": block_hash,
            "transaction_count": int(count),
            "raw_block_bytes": int(byte_size),
            "workers": args.workers,
            "repetitions_per_process": args.repeats,
            "paired_process_trials": args.trials,
            "tree_median_ms": round(tree_ms, 3),
            "hash_median_ms": round(hash_ms, 3),
            "tree_over_hash_ratio": round(tree_ms / hash_ms, 4),
            "tree_peak_rss_kib": median_metric("tree", "process_peak_rss_kib"),
            "hash_peak_rss_kib": median_metric("hash", "process_peak_rss_kib"),
            "tree_peak_working_set_kib": median_metric("tree", "process_peak_working_set_kib"),
            "hash_peak_working_set_kib": median_metric("hash", "process_peak_working_set_kib"),
            "tree_ms_samples": sorted(float(row["prepared_only_median_ms"]) for row in measurements["tree"]),
            "hash_ms_samples": sorted(float(row["prepared_only_median_ms"]) for row in measurements["hash"]),
            "source_trust": "LOCAL_MERKLE_ONLY",
        }
        print(json.dumps(record, sort_keys=True))


if __name__ == "__main__":
    main()
