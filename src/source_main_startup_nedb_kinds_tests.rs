// Genuine NeDB 1.8 order for the Incoming lane's request kinds (upsert, plain
// update, field findOne) across idle databases. The frozen reference is a
// comparison only; found/not-found comes from each scene's declared documents.
mod nedb_kinds {
    use super::*;
    use crate::source_main_step_kernel::startup_source_nedb_jobs::RequestKey;
    fn genuine(section: &str) -> Vec<String> {
        let text = include_str!("../fixtures/startup_nedb_kinds_reference.json");
        let body = &text[text.find(&format!("\"{section}\"")).unwrap()..];
        let body = &body[body.find('[').unwrap() + 1..body.find(']').unwrap()];
        body.split('"')
            .skip(1)
            .step_by(2)
            .map(str::to_owned)
            .collect()
    }
    struct Scene {
        promises: StartupPromiseQueue,
        nedb: StartupNedbQueue,
        requests: Vec<(RequestKey, &'static str, Collection, QueryKind, bool)>,
        sampled_once: Vec<RequestKey>,
        events: Vec<String>,
        ordinal: u64,
    }
    impl Scene {
        fn new() -> Self {
            Self {
                promises: StartupPromiseQueue::new(9).unwrap(),
                nedb: StartupNedbQueue::new(9).unwrap(),
                requests: vec![],
                sampled_once: vec![],
                events: vec![],
                ordinal: 0,
            }
        }
        fn request(
            &mut self,
            label: &'static str,
            collection: Collection,
            kind: QueryKind,
            found: bool,
        ) {
            self.ordinal += 1;
            let payload = reserve(&mut self.promises, self.ordinal);
            let key = self
                .nedb
                .enqueue_actual_request(&self.promises, payload, collection, kind)
                .unwrap();
            self.requests.push((key, label, collection, kind, found));
        }
        fn run(&mut self, mut on_callback: impl FnMut(&mut Self, &str)) {
            while let Some(delivery) = self.nedb.next_delivery().unwrap() {
                let (_, label, collection, kind, found) = *self
                    .requests
                    .iter()
                    .find(|(key, ..)| *key == delivery.request)
                    .unwrap();
                match delivery.kind {
                    DeliveryKind::SampleCandidates => {
                        self.events.push(format!("sample:{}", collection.name()));
                        if kind == QueryKind::Upsert
                            && !self.sampled_once.contains(&delivery.request)
                        {
                            self.sampled_once.push(delivery.request);
                            assert_eq!(
                                self.nedb.acknowledge_actual(delivery),
                                Err(SourceRuleError::InvalidInput),
                                "an existence sample needs its actual outcome"
                            );
                            self.nedb
                                .acknowledge_upsert_sample(delivery, found)
                                .unwrap();
                        } else {
                            self.nedb.acknowledge_actual(delivery).unwrap();
                        }
                    }
                    DeliveryKind::DeliverCallback => {
                        self.nedb.acknowledge_actual(delivery).unwrap();
                        self.events.push(format!("callback:{label}"));
                        on_callback(self, label);
                    }
                }
            }
        }
    }
    #[test]
    fn genuine_upsert_and_field_query_order_across_idle_databases() {
        let mut scene = Scene::new();
        scene.request(
            "txIndex-upsert",
            Collection::TxIndex,
            QueryKind::Upsert,
            false,
        );
        scene.request(
            "persistence-findOne-height",
            Collection::Persistence,
            QueryKind::FindOneQuery,
            true,
        );
        scene.request(
            "consensus-findOne",
            Collection::Consensus,
            QueryKind::FindOneStringId,
            false,
        );
        scene.request(
            "txIndex-upsert-2",
            Collection::TxIndex,
            QueryKind::Upsert,
            false,
        );
        scene.run(|scene, label| {
            if label == "txIndex-upsert" {
                scene.request(
                    "consensus-upsert",
                    Collection::Consensus,
                    QueryKind::Upsert,
                    false,
                );
                scene.request(
                    "txIndex-findOne",
                    Collection::TxIndex,
                    QueryKind::FindOneStringId,
                    false,
                );
            }
        });
        assert_eq!(scene.events, genuine("updateAndFieldQueries"));
    }
    #[test]
    fn genuine_modifying_upsert_takes_a_second_candidate_pass() {
        let mut scene = Scene::new();
        scene.request(
            "consensus-upsert-modify",
            Collection::Consensus,
            QueryKind::Upsert,
            true,
        );
        scene.request(
            "persistence-update-modify",
            Collection::Persistence,
            QueryKind::Update,
            true,
        );
        scene.request(
            "txIndex-findOne",
            Collection::TxIndex,
            QueryKind::FindOneStringId,
            true,
        );
        scene.run(|_, _| {});
        assert_eq!(scene.events, genuine("modifyPaths"));
        // The modifying upsert sampled twice and was acknowledged once per pass.
        assert_eq!(
            scene
                .events
                .iter()
                .filter(|e| *e == "sample:consensus")
                .count(),
            2
        );
    }
}
