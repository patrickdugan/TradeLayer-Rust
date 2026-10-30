pub use tradelayer_core::validity_source;
#[allow(dead_code)]
#[path = "../src/source_main_step_kernel.rs"]
mod kernel;
#[allow(dead_code)]
#[path = "../src/source_main_startup_prefix.rs"]
mod source_main_startup_prefix;
use kernel::*;
use std::collections::BTreeSet;
use validity_source::{SourceRuleError, SourceValue as V};

#[test]
fn complete_argument_and_trace_streams_exceed_old_aggregate_bounds_without_truncation() {
    let row = V::Array((0..1024).map(|at| V::number(at as f64)).collect());
    row.validate().unwrap();
    let command = MainStepCommandV1::Start {
        method: "Main.setPause".into(),
        arguments: vec![row; 8],
    };
    assert_eq!(command.value().validate(), Err(SourceRuleError::Capacity));
    let mut last = None;
    let mut seen = BTreeSet::new();
    command
        .visit_canonical_fields(&mut |path, value| {
            value.validate()?;
            assert!(seen.insert(path.to_string()));
            if path == "call/arguments/7/items/1023/atom" {
                last = Some(value.clone());
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(last, Some(V::number(1023.)));

    let evaluation = MainStepEvaluationV1 {
        started: None,
        stop: MainStepStop::DrainBound,
        trace: (0..1700)
            .map(|at| MainStepTraceV1 {
                job: at,
                phase: "actual typed trace metadata".into(),
                operation: V::Undefined,
                outcome: V::Null,
                effects: vec![],
            })
            .collect(),
        effects: vec![],
        completed: vec![],
    };
    assert_eq!(
        evaluation.value().validate(),
        Err(SourceRuleError::Capacity)
    );
    let mut last = None;
    evaluation
        .visit_canonical_fields(&mut |path, value| {
            value.validate()?;
            if path == "trace/1699/job" {
                last = Some(value.clone());
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(last, Some(V::String("1699".into())));
}

#[test]
fn ordered_structural_fields_disambiguate_keys_optionals_and_source_scalar_types() {
    fn encoded(value: V) -> Vec<(String, V)> {
        let command = MainStepCommandV1::UpdateRecordedIo(value);
        let mut out = vec![];
        command
            .visit_canonical_fields(&mut |path, value| {
                out.push((path.into(), value.clone()));
                Ok(())
            })
            .unwrap();
        out
    }
    let left = V::Object(vec![("a/b".into(), V::Undefined), ("a".into(), V::Null)]);
    let right = V::Object(vec![("a/b".into(), V::Null), ("a".into(), V::Undefined)]);
    assert_ne!(encoded(left), encoded(right));
    assert_ne!(encoded(V::Array(vec![])), encoded(V::Object(vec![])));
    assert_ne!(encoded(V::number(-0.)), encoded(V::number(0.)));
    let kernel = MainStepKernelV1::new(u64::MAX);
    let before = kernel.value();
    let mut visited = 0;
    assert_eq!(
        kernel.visit_canonical_fields(&mut |_, _| {
            visited += 1;
            if visited == 7 {
                Err(SourceRuleError::Capacity)
            } else {
                Ok(())
            }
        }),
        Err(SourceRuleError::Capacity)
    );
    assert_eq!(kernel.value(), before, "sink refusal is read-only");
    let mut generation = None;
    kernel
        .visit_canonical_fields(&mut |path, value| {
            if path == "generation" {
                generation = Some(value.clone());
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(generation, Some(V::String(u64::MAX.to_string())));
}
