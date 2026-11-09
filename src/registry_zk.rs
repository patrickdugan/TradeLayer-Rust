// `ZkVerifyV1` registry state: programs, receipts, the type-36 state transition and the v2
// snapshot section. A child module of `registry` so it reads the private fields.
use super::*;
use crate::codec::Cursor;
use crate::ledger::PropertyId;
use crate::zk_verify::{
    binding_element, check_nova_key, program_id_of, public_inputs_hash, split_proof_envelope,
    ZkBindingContext, ZkBlockBudget, ZkBudgetError, ZkOutcome, ZkRejectReason, ZkVerifier,
    ZkVerifyPayload, MAX_ZK_BLOCK_VERIFY_MS, MAX_ZK_KEY_BYTES, MAX_ZK_PROGRAMS,
    MAX_ZK_PUBLIC_INPUTS, MAX_ZK_RECEIPTS, MAX_ZK_TX_PROOF_BYTES,
};

impl Registry {
    /// Opts this registry into `ZkVerifyV1`. Allowed only on a registry that has processed no
    /// transaction, so the policy is part of the chain's configuration from its first block.
    pub fn enable_zk_verify_v1(&mut self, fee: ZkFeePolicy) -> Result<(), RegistryError> {
        if self.zk_fee.is_some() || !self.processed.is_empty() || self.last_height.is_some() {
            return Err(RegistryError::ZkVerifyDisabled);
        }
        self.zk_fee = Some(fee);
        Ok(())
    }

    pub fn new_zk_verify_v1(protocol_admin: AccountId, fee: ZkFeePolicy) -> Self {
        let mut registry = Self::new(protocol_admin);
        registry.zk_fee = Some(fee);
        registry
    }

    pub fn zk_policy(&self) -> Option<&ZkFeePolicy> {
        self.zk_fee.as_ref()
    }

    pub fn zk_program(&self, program_id: &[u8; 32]) -> Option<&ZkProgram> {
        self.zk_programs.get(program_id)
    }

    pub fn zk_program_count(&self) -> usize {
        self.zk_programs.len()
    }

    pub fn zk_receipts(&self) -> &[StatementReceipt] {
        &self.zk_receipts
    }

    pub fn zk_receipts_for_program<'a>(
        &'a self,
        program_id: &'a [u8; 32],
    ) -> impl Iterator<Item = &'a StatementReceipt> {
        self.zk_receipts
            .iter()
            .filter(move |r| &r.program_id == program_id)
    }

    pub fn zk_receipts_for_sender<'a>(
        &'a self,
        sender: &'a AccountId,
    ) -> impl Iterator<Item = &'a StatementReceipt> {
        self.zk_receipts.iter().filter(move |r| &r.sender == sender)
    }

    pub(super) fn register_zk_program(&mut self, program: ZkProgram) -> Result<(), RegistryError> {
        if program.system != ZkSystem::Nova {
            return Err(RegistryError::ZkSystemReserved);
        }
        if program.key_bytes.is_empty() || program.key_bytes.len() > MAX_ZK_KEY_BYTES {
            return Err(RegistryError::InvalidField("zk key bytes"));
        }
        // Index 0 is the binding element, so every layout has at least one element.
        if program.public_input_layout == 0
            || program.public_input_layout as usize > MAX_ZK_PUBLIC_INPUTS
        {
            return Err(RegistryError::InvalidField("zk public input layout"));
        }
        if check_nova_key(&program.key_bytes, program.public_input_layout).is_err() {
            return Err(RegistryError::InvalidField("zk nova key"));
        }
        if program.verifier_version == 0 {
            return Err(RegistryError::InvalidField("zk verifier version"));
        }
        if program.max_verify_ms == 0 || u64::from(program.max_verify_ms) > MAX_ZK_BLOCK_VERIFY_MS {
            return Err(RegistryError::InvalidField("zk max verify ms"));
        }
        let expected = program_id_of(
            program.system,
            &program.key_bytes,
            program.public_input_layout,
            program.effect,
            program.verifier_version,
            program.max_verify_ms,
        );
        if expected != program.program_id {
            return Err(RegistryError::ZkProgramIdMismatch);
        }
        if self.zk_programs.contains_key(&program.program_id) {
            return Err(RegistryError::DuplicateZkProgram);
        }
        if self.zk_programs.len() >= MAX_ZK_PROGRAMS {
            return Err(RegistryError::ZkRegistryFull);
        }
        self.zk_programs.insert(program.program_id, program);
        Ok(())
    }

    /// Hard preconditions of a type-36 transaction. Failing here is an invalid transaction (no
    /// fee, no replay marker), like any other type before activation.
    pub fn zk_gate(&self, height: u64) -> Result<(), RegistryError> {
        if self.zk_fee.is_none() {
            return Err(RegistryError::ZkVerifyDisabled);
        }
        self.check_active(ZK_TX_TYPE, height)
    }

    /// Records one type-36 transaction whose fee the caller has already debited. Rejections are
    /// outcomes, not errors: the transaction is consumed (replay marker recorded), no receipt is
    /// written, and the caller keeps the fee. `Err` leaves the registry untouched.
    #[allow(clippy::too_many_arguments)]
    pub fn record_zk_verify(
        &mut self,
        txid: TxId,
        actor: &AccountId,
        height: u64,
        payload: &ZkVerifyPayload,
        ctx: &ZkBindingContext,
        budget: &mut ZkBlockBudget,
        verifier: &dyn ZkVerifier,
    ) -> Result<ZkOutcome, RegistryError> {
        self.zk_gate(height)?;
        if height == 0 {
            return Err(RegistryError::InvalidHeight);
        }
        if self.last_height.is_some_and(|prior| height < prior) {
            return Err(RegistryError::HeightRegression);
        }
        if self.processed.contains(&txid) {
            return Err(RegistryError::Replay(txid));
        }
        self.processed.insert(txid);
        self.last_height = Some(height);
        let outcome = self.zk_outcome(txid, actor, height, payload, ctx, budget, verifier);
        if let ZkOutcome::Verified(receipt) = &outcome {
            self.zk_receipts.push(receipt.clone());
        }
        Ok(outcome)
    }

    #[allow(clippy::too_many_arguments)]
    fn zk_outcome(
        &self,
        txid: TxId,
        actor: &AccountId,
        height: u64,
        payload: &ZkVerifyPayload,
        ctx: &ZkBindingContext,
        budget: &mut ZkBlockBudget,
        verifier: &dyn ZkVerifier,
    ) -> ZkOutcome {
        use ZkRejectReason as R;
        let reject = ZkOutcome::Rejected;
        if height > payload.expiry {
            return reject(R::Expired);
        }
        let Some(program) = self.zk_programs.get(&payload.program_id) else {
            return reject(R::UnknownProgram);
        };
        if payload.system != program.system.code() {
            return reject(R::SystemMismatch);
        }
        if payload.proof.len() > MAX_ZK_TX_PROOF_BYTES {
            return reject(R::OversizeProof);
        }
        if payload.public_inputs.len() != program.public_input_layout as usize {
            return reject(R::PublicInputCount);
        }
        if self.zk_receipts.len() >= MAX_ZK_RECEIPTS {
            return reject(R::ReceiptsExhausted);
        }
        let binding = binding_element(ctx, actor, payload.expiry, &payload.program_id);
        if payload.public_inputs.first() != Some(&binding) {
            return reject(R::BindingMismatch);
        }
        let Some((num_steps, proof)) = split_proof_envelope(&payload.proof) else {
            return reject(R::ProofEnvelope);
        };
        if let Err(error) = budget.charge(payload.proof.len(), program.max_verify_ms) {
            return reject(match error {
                ZkBudgetError::Count => R::BudgetCount,
                ZkBudgetError::ProofBytes => R::BudgetProofBytes,
                ZkBudgetError::VerifyMs => R::BudgetVerifyMs,
            });
        }
        let hash = public_inputs_hash(&payload.public_inputs);
        match verifier.verify(
            program.system,
            &program.key_bytes,
            num_steps,
            &payload.public_inputs,
            proof,
        ) {
            Err(error) => reject(R::Verifier(error)),
            Ok(statement) if statement.public_inputs_hash != hash => reject(R::StatementMismatch),
            Ok(_) => match program.effect {
                ZkEffect::StatementReceipt => ZkOutcome::Verified(StatementReceipt {
                    program_id: program.program_id,
                    public_inputs_hash: hash,
                    sender: actor.clone(),
                    height,
                    txid,
                }),
            },
        }
    }

    /// Snapshot v2 tail: fee policy, type-36 activation, programs, receipts.
    pub(super) fn put_zk_section(&self, out: &mut Vec<u8>) -> Result<(), RegistrySnapshotError> {
        let fee = self
            .zk_fee
            .as_ref()
            .ok_or_else(|| err("zk policy absent"))?;
        out.extend_from_slice(&fee.property.get().to_be_bytes());
        out.extend_from_slice(&fee.amount_atoms.to_be_bytes());
        put_account(out, &fee.sink)?;
        match &self.zk_activation {
            None => out.push(0),
            Some(a) => {
                out.push(1);
                out.extend_from_slice(&a.height.to_be_bytes());
                out.extend_from_slice(&a.code_hash);
            }
        }
        put_u32(out, self.zk_programs.len())?;
        for p in self.zk_programs.values() {
            out.push(p.system.code());
            out.extend_from_slice(&p.program_id);
            put_u32(out, p.key_bytes.len())?;
            out.extend_from_slice(&p.key_bytes);
            out.extend_from_slice(&p.public_input_layout.to_be_bytes());
            out.push(p.effect.code());
            out.extend_from_slice(&p.verifier_version.to_be_bytes());
            out.extend_from_slice(&p.max_verify_ms.to_be_bytes());
        }
        put_u32(out, self.zk_receipts.len())?;
        for r in &self.zk_receipts {
            out.extend_from_slice(&r.program_id);
            out.extend_from_slice(&r.public_inputs_hash);
            put_account(out, &r.sender)?;
            out.extend_from_slice(&r.height.to_be_bytes());
            out.extend_from_slice(&r.txid.as_bytes());
        }
        Ok(())
    }

    pub(super) fn read_zk_section(
        &mut self,
        r: &mut Cursor<'_>,
    ) -> Result<(), RegistrySnapshotError> {
        let property = PropertyId::try_from(r.u32().map_err(|_| err("zk fee property"))?)
            .map_err(|_| err("zk fee property"))?;
        let amount_atoms = r.u128().map_err(|_| err("zk fee amount"))?;
        let sink = read_account(r)?;
        self.zk_fee = Some(ZkFeePolicy {
            property,
            amount_atoms,
            sink,
        });
        if read_flag(r)? {
            let height = r.u64().map_err(|_| err("zk activation height"))?;
            let code_hash = r.fixed::<32>().map_err(|_| err("zk activation hash"))?;
            if height == 0 {
                return Err(err("invalid zk activation"));
            }
            self.zk_activation = Some(Activation { height, code_hash });
        }
        let programs = read_count(r)?;
        if programs > MAX_ZK_PROGRAMS {
            return Err(err("zk program count exceeds limit"));
        }
        for _ in 0..programs {
            let system = ZkSystem::from_code(r.u8().map_err(|_| err("zk system"))?)
                .ok_or_else(|| err("zk system"))?;
            let program_id = r.fixed::<32>().map_err(|_| err("zk program id"))?;
            let key_len = r.u32().map_err(|_| err("zk key length"))? as usize;
            if key_len > MAX_ZK_KEY_BYTES {
                return Err(err("zk key too long"));
            }
            let key_bytes = r.take(key_len).map_err(|_| err("zk key"))?.to_vec();
            let public_input_layout = r.u32().map_err(|_| err("zk layout"))?;
            let effect = ZkEffect::from_code(r.u8().map_err(|_| err("zk effect"))?)
                .ok_or_else(|| err("zk effect"))?;
            let verifier_version = r.u32().map_err(|_| err("zk verifier version"))?;
            let max_verify_ms = r.u32().map_err(|_| err("zk max verify ms"))?;
            let program = ZkProgram {
                system,
                program_id,
                key_bytes,
                public_input_layout,
                effect,
                verifier_version,
                max_verify_ms,
            };
            self.register_zk_program(program)
                .map_err(|_| err("invalid zk program"))?;
        }
        let receipts = read_count(r)?;
        if receipts > MAX_ZK_RECEIPTS {
            return Err(err("zk receipt count exceeds limit"));
        }
        for _ in 0..receipts {
            let program_id = r.fixed::<32>().map_err(|_| err("receipt program"))?;
            let public_inputs_hash = r.fixed::<32>().map_err(|_| err("receipt hash"))?;
            let sender = read_account(r)?;
            let height = r.u64().map_err(|_| err("receipt height"))?;
            let txid = TxId::from_bytes(r.fixed::<32>().map_err(|_| err("receipt txid"))?);
            if height == 0 || !self.zk_programs.contains_key(&program_id) {
                return Err(err("invalid zk receipt"));
            }
            self.zk_receipts.push(StatementReceipt {
                program_id,
                public_inputs_hash,
                sender,
                height,
                txid,
            });
        }
        Ok(())
    }

    /// Cross-checks run after the whole snapshot is read.
    pub(super) fn zk_history_consistent(&self, latest: u64) -> bool {
        self.zk_activation.is_none_or(|a| a.height <= latest)
            && self
                .zk_receipts
                .iter()
                .all(|r| r.height <= latest && self.processed.contains(&r.txid))
            && self
                .zk_receipts
                .windows(2)
                .all(|w| w[0].height <= w[1].height)
    }
}
