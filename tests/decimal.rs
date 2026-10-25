use core::cmp::Ordering;
use tradelayer_core::decimal::{Decimal, Rounding};

#[test]
fn actual_bignumber_arithmetic_golden_vectors() {
    let mut count = 0;
    for line in include_str!("../fixtures/decimal_vectors.tsv").lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 6);
        let [method, left, right, places, rounding, expected] =
            <[&str; 6]>::try_from(fields).unwrap();
        let a = left.parse::<Decimal>().unwrap();
        let b = (right != "-").then(|| right.parse::<Decimal>().unwrap());
        let mode = match rounding {
            "down" => Rounding::Down,
            "half-up" => Rounding::HalfUp,
            _ => panic!("unknown rounding"),
        };
        let places = places.parse().unwrap();
        let result = match method {
            "parse" => a.to_string(),
            "add" => a.checked_add(b.as_ref().unwrap()).unwrap().to_string(),
            "sub" => a.checked_sub(b.as_ref().unwrap()).unwrap().to_string(),
            "mul" => a.checked_mul(b.as_ref().unwrap()).unwrap().to_string(),
            "div" => a
                .checked_div(b.as_ref().unwrap(), places, mode)
                .unwrap()
                .to_string(),
            "quantize" => a.quantize(places, mode).unwrap().to_string(),
            "cmp" => match a.cmp(b.as_ref().unwrap()) {
                Ordering::Less => "-1",
                Ordering::Equal => "0",
                Ordering::Greater => "1",
            }
            .to_owned(),
            _ => panic!("unknown operation"),
        };
        assert_eq!(
            result, expected,
            "{method}: {left}, {right}, places={places}, mode={rounding}"
        );
        count += 1;
    }
    assert_eq!(count, 1942);
}
