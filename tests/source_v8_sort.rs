#![deny(warnings)]
include!("../src/source_v8_sort_impl.rs");

#[test]
fn exact_v8_order_and_comparison_schedule() {
    use core::cmp::Ordering;
    let input = if let Ok(path) = std::env::var("SOURCE_SORT_VECTORS") {
        std::fs::read_to_string(path).unwrap()
    } else {
        let output = std::process::Command::new("node")
            .args([
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/scripts/source_v8_sort_vectors.cjs"
                ),
                "--print",
            ])
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    };
    let mut cases = 0;
    for line in input.lines() {
        let fields: Vec<_> = line.split('|').collect();
        let descending = fields[2] == "1";
        let mut rows: Vec<(usize, f64)> = fields[3]
            .split(',')
            .filter(|s| !s.is_empty())
            .enumerate()
            .map(|(i, n)| (i, f64::from_bits(u64::from_str_radix(n, 16).unwrap())))
            .collect();
        let mut trace = Vec::new();
        source_v8_stable_sort(&mut rows, |a, b| -> Result<Ordering, ()> {
            trace.push(format!("{},{}", a.0, b.0));
            Ok((if descending { b.1 - a.1 } else { a.1 - b.1 })
                .partial_cmp(&0.)
                .unwrap_or(Ordering::Equal))
        })
        .unwrap();
        let order = rows
            .iter()
            .map(|x| x.0.to_string())
            .collect::<Vec<_>>()
            .join(",");
        assert_eq!(order, fields[4], "{} order", fields[1]);
        assert_eq!(
            trace.join(";"),
            fields[5],
            "{} comparator schedule",
            fields[1]
        );
        cases += 1;
    }
    assert_eq!(cases, 44890);
}

#[test]
fn comparator_errors_do_not_publish_work_array() {
    for length in [4, 64, 65, 128, 1024] {
        let initial: Vec<_> = (0..length).map(|i| (i * 67) % 127).collect();
        for fail_at in [1, 2, 4, 16, 32, 64, 128, 512] {
            let mut rows = initial.clone();
            let mut seen = 0;
            let result = source_v8_stable_sort(&mut rows, |a, b| {
                seen += 1;
                if seen == fail_at {
                    Err("source comparator failure")
                } else {
                    Ok(a.cmp(b))
                }
            });
            if result.is_err() {
                assert_eq!(rows, initial);
            }
        }
    }
}
