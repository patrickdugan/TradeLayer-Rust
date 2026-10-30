// The same Kernel's Promise queue and NeDB executor drive the actual outer frame.
// Genuine source request order is a comparison only, never a schedule input.
fn outer_request_text(value: &V) -> String {
    match value {
        V::String(text) => format!("\"{text}\""),
        V::Array(items) => format!(
            "[{}]",
            items
                .iter()
                .map(outer_request_text)
                .collect::<Vec<_>>()
                .join(",")
        ),
        V::Object(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(key, item)| format!("\"{key}\":{}", outer_request_text(item)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        other => panic!("unexpected outer request argument {other:?}"),
    }
}
fn kernel_driven(
    runtime: &mut SourceContractRuntime,
    index: &SourceCompactIndex,
) -> (NativeMainOuterKernelDriverV1, NativeMainOuterBootstrapV1) {
    let (mut driver, mut frame) =
        NativeMainOuterKernelDriverV1::new(runtime, n(10.), index, 9).unwrap();
    driver.run(runtime, &mut frame, index).unwrap();
    (driver, frame)
}
#[test]
fn kernel_driven_outer_prefix_issues_requests_in_genuine_source_order() {
    let temp = Temp::new();
    let (mut runtime, index) = seeded(&temp);
    let (driver, frame) = kernel_driven(&mut runtime, &index);
    let actual: String = driver
        .requests()
        .iter()
        .map(|request| {
            format!(
                "{}\t{}\t{}\n",
                request.collection,
                request.method.trim_end_matches("Async"),
                outer_request_text(&V::Array(request.arguments.clone()))
            )
        })
        .collect();
    // All four genuine Main.initialize runs (both numeric profiles), at both
    // constructConsensusFromIndex prefixes, issue exactly this order.
    assert_eq!(
        actual,
        include_str!("../fixtures/startup_outer_request_order_genuine.txt")
    );
    assert!(frame.prepared_ready());
    assert!(frame.tally_child_complete());
    assert!(matches!(
        frame.actual_tally_instance_completion(),
        Some(NativeMainOuterActualCompletionV1::Value(_))
    ));
    assert!(!runtime.core.tally.is_empty(), "actual Tally load ran");
    assert_eq!(driver.tally_job(), Some(2));
    assert!(frame.stopped().is_none());
}
#[test]
fn kernel_driven_tally_factory_reaction_precedes_consensus_factory_reaction() {
    use NativeMainOuterBootstrapSiteV1 as Site;
    let temp = Temp::new();
    let (mut runtime, index) = seeded(&temp);
    let (driver, _) = kernel_driven(&mut runtime, &index);
    let sites: Vec<_> = driver
        .resumed()
        .iter()
        .map(|await_| await_.site())
        .collect();
    let at = |site| sites.iter().position(|s| *s == site).unwrap();
    // Both factories complete in one source segment with no drain between them;
    // their reactions then run in the Kernel's FIFO order.
    assert!(at(Site::TxIndexFactory) < at(Site::TallyFactory));
    assert!(at(Site::TallyFactory) < at(Site::ConsensusFactory));
    assert!(at(Site::TallyQuery) < at(Site::MaxProcessedQuery));
    assert_eq!(sites.last(), Some(&Site::FixedRead));
    assert_eq!(
        sites
            .iter()
            .filter(|site| **site == Site::TallyLoadResult)
            .count(),
        1
    );
    let mut fields = BTreeMap::new();
    driver
        .kernel()
        .visit_canonical_fields(&mut |path, value| {
            fields.insert(path.to_owned(), value.clone());
            Ok(())
        })
        .unwrap();
    assert_eq!(
        fields.get("sourceProfile"),
        Some(&s("TL-MAIN-VM-BUSINESS-HOST-IO-13-1"))
    );
    assert!(fields
        .keys()
        .any(|path| path.starts_with("startupPromises/")));
    assert!(fields.keys().any(|path| path.starts_with("startupNedb/")));
}
#[test]
fn kernel_driven_and_component_driven_frames_retain_the_same_actual_results() {
    use NativeMainOuterBootstrapLaneV1 as Lane;
    let kernel_temp = Temp::new();
    let (mut kernel_runtime, kernel_index) = seeded(&kernel_temp);
    let (_, mut kernel_frame) = kernel_driven(&mut kernel_runtime, &kernel_index);
    let temp = Temp::new();
    let (mut runtime, index) = seeded(&temp);
    let mut frame = through_two_factories(&mut runtime, &index);
    resume(&mut runtime, &mut frame, &index, Lane::Main); // consensus factory
    query(&mut runtime, &mut frame, &index, Lane::Main); // MaxProcessed query + reaction
    resume(&mut runtime, &mut frame, &index, Lane::Main); // MaxProcessed result
    query(&mut runtime, &mut frame, &index, Lane::Main); // fixed read + plan
    resume(&mut runtime, &mut frame, &index, Lane::Tally); // Tally factory
    query(&mut runtime, &mut frame, &index, Lane::Tally); // Tally query + reaction
    resume(&mut runtime, &mut frame, &index, Lane::Tally); // Tally load result
                                                           // Scheduling differs; the actual retained results and plan do not.
    assert_eq!(kernel_runtime.core.tally, runtime.core.tally);
    let kernel_plan = kernel_frame.take_prepared().unwrap().unwrap().unwrap();
    let plan = frame.take_prepared().unwrap().unwrap().unwrap();
    assert_eq!(kernel_plan.last, plan.last);
    // Separate on-disk indexes have distinct manifest identities; the streamed
    // row content, its chain and completeness must agree.
    let (a, b) = (kernel_plan.plan.receipt(), plan.plan.receipt());
    assert_eq!(
        (
            &a.profile,
            a.snapshot.sequence,
            a.rows,
            a.chain_sha256d,
            a.complete
        ),
        (
            &b.profile,
            b.snapshot.sequence,
            b.rows,
            b.chain_sha256d,
            b.complete
        )
    );
    assert!(a.complete && a.rows > 0);
}
#[test]
fn kernel_hooks_refuse_a_foreign_parent_or_a_second_tally_child() {
    let mut kernel = MainStepKernelV1::new(9);
    kernel.startup_install_source_promises().unwrap();
    let (promises, _) = kernel.startup_source_parts().unwrap();
    let main_job = promises
        .allocate_source_job("Main.constructConsensusFromIndex")
        .unwrap();
    let mut routes = vec![];
    let mut tally_job = None;
    let mut hooks = NativeMainOuterKernelHooksV1 {
        promises,
        routes: &mut routes,
        main_job,
        tally_job: &mut tally_job,
    };
    let scope = "TallyMap.getInstance/loadFromDB";
    assert_eq!(
        hooks.register_child(main_job.serial() + 7, scope),
        Err(SourceRuleError::InvalidInput)
    );
    let child = hooks.register_child(main_job.serial(), scope).unwrap();
    assert_ne!(child, main_job.serial());
    assert_eq!(
        hooks.register_child(main_job.serial(), scope),
        Err(SourceRuleError::InvalidInput)
    );
    assert_eq!(tally_job.map(SourceJobKey::serial), Some(child));
}
