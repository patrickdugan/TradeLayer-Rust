// Included in economic.rs. `ZkVerifyV1` on the source path: the policy gate, the registry
// binding and the application of one ZK row on a staged block. A ZK row is a source row whose
// payload starts with `zk_verify::ZK_ROW_PREFIX` (see `docs/ZK_VERIFIER.md`).

impl EconomicState {
    /// Opts the pristine owner's registry into `ZkVerifyV1` when the source config asks for it.
    /// The retained bootstrap stays the pristine bytes the deployment pins.
    fn enable_zk_policy_from_config(&mut self) -> Result<()> {
        if let Some(fee) = self
            .config
            .source_consensus
            .as_ref()
            .and_then(|s| s.zk_verify.clone())
        {
            self.joint
                .enable_zk_verify_v1(fee)
                .map_err(|_| EconomicError::InvalidConfig)?;
        }
        Ok(())
    }

    /// Under a source config the registry carries the policy exactly when the config does, with
    /// the same fee. The source-free owner embedded in a source checkpoint is exempt: it has no
    /// source config and its registry holds whatever the full state's did.
    fn verify_zk_policy(&self) -> Result<()> {
        let Some(source) = self.config.source_consensus.as_ref() else {
            return Ok(());
        };
        if self.joint.registry().zk_policy() == source.zk_verify.as_ref() {
            Ok(())
        } else {
            Err(EconomicError::Invariant)
        }
    }

    /// Applies a block whose rows may include ZK rows, running `verifier` on each proof. The
    /// owner records the verdicts in the block's receipts; a rebuild takes them from there.
    pub fn apply_source_block_zk_unverified(
        &mut self,
        header: SourceHeader,
        mode: SourceBlockMode,
        transactions: &[SourceTransaction],
        verifier: &dyn crate::zk_verify::ZkVerifier,
    ) -> core::result::Result<SourceBlockReport, consensus_source::ConsensusSourceError> {
        let (next, report) = consensus_source::execute_block_unverified(
            self,
            header,
            mode,
            transactions,
            consensus_source::ZkExec::Live(verifier),
        )?;
        *self = next;
        Ok(report)
    }
}

impl EconomicBlockStage {
    /// Applies one ZK row to the staged block and returns its receipt disposition and reason.
    ///
    /// Order, as on the joint path: activation gate, fee debit (kept on every later rejection),
    /// then budget, binding and the verifier. A row that cannot be decoded, or whose transaction
    /// is invalid (not activated, fee unpayable, replayed), changes nothing and is recorded;
    /// `Applied` means a receipt, a registered program or the activation was written.
    pub(crate) fn apply_zk_row_unverified(
        &mut self,
        tx: &SourceTransaction,
        ctx: &crate::zk_verify::ZkBindingContext,
        budget: &mut crate::zk_verify::ZkBlockBudget,
        verifier: &dyn crate::zk_verify::ZkVerifier,
    ) -> (SourceDisposition, String) {
        use crate::joint::{JointOperation, UnverifiedJointZkTx};
        use crate::zk_verify::{
            parse_zk_row_payload, zk_reason_rejected, ZkNodeFrame, ZkOutcome, ZK_REASON_VERIFIED,
        };
        let invalid = |why: &str| (SourceDisposition::Invalid, format!("zk:invalid:{why}"));
        let Some(raw) = parse_zk_row_payload(&tx.payload) else {
            return invalid("payload");
        };
        let Ok(frame) = ZkNodeFrame::decode(&raw) else {
            return invalid("frame");
        };
        let SourceValue::String(address) = &tx.sender else {
            return invalid("sender");
        };
        let Ok(actor) = AccountId::try_from(address.as_str()) else {
            return invalid("sender");
        };
        if normal_owner(&actor).is_err() {
            return invalid("sender");
        }
        let height = self.height;
        let mut joint = self.state.joint.clone();
        let mut trial = budget.clone();
        let applied = match frame {
            ZkNodeFrame::Verify(payload) => joint
                .apply_zk_for_economic_unverified(
                    height,
                    UnverifiedJointZkTx {
                        txid: tx.txid,
                        actor,
                        payload,
                    },
                    ctx,
                    &mut trial,
                    verifier,
                )
                .map(|outcome| match outcome {
                    ZkOutcome::Verified(_) => {
                        (SourceDisposition::Applied, ZK_REASON_VERIFIED.into())
                    }
                    ZkOutcome::Rejected(reason) => (
                        SourceDisposition::EconomicRejected,
                        zk_reason_rejected(&reason),
                    ),
                }),
            ZkNodeFrame::Register(payload) => {
                let Some(action) = payload.into_action() else {
                    return invalid("register");
                };
                joint
                    .apply_one_for_economic_unverified(
                        height,
                        UnverifiedJointTx {
                            txid: tx.txid,
                            actor,
                            operation: JointOperation::Registry(action),
                        },
                    )
                    .map(|()| (SourceDisposition::Applied, "zk:registered".to_owned()))
            }
            ZkNodeFrame::Activate(payload) => joint
                .apply_one_for_economic_unverified(
                    height,
                    UnverifiedJointTx {
                        txid: tx.txid,
                        actor,
                        operation: JointOperation::Registry(payload.into_action()),
                    },
                )
                .map(|()| (SourceDisposition::Applied, "zk:activated".to_owned())),
        };
        match applied {
            Ok(result) => {
                self.state.joint = joint;
                *budget = trial;
                result
            }
            Err(error) => (
                SourceDisposition::EconomicRejected,
                format!("zk:invalid:{error:?}"),
            ),
        }
    }
}
