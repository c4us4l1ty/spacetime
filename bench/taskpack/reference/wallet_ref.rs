//! Reference fake-credit wallet for the Block 3 EconBench task pack.
//!
//! This is the post-fix solution. It implements the ABI specification exactly, is written without
//! relying on any candidate code, and is graded by `oracle/wallet_oracle.rs`.

pub mod wallet {
    pub struct Wallet {
        balances: Vec<(String, u64)>,
        applied: Vec<(String, u64)>,
        events: Vec<CandidateEvent>,
    }

    impl Wallet {
        pub fn new(accounts: &[(&str, u64)]) -> Result<Wallet, WalletError> {
            let mut balances: Vec<(String, u64)> = Vec::new();
            for (account, initial) in accounts {
                validate_identifier(account)?;
                if balances.iter().any(|(name, _)| name.as_str() == *account) {
                    return Err(WalletError::DuplicateAccount);
                }
                balances.push(((*account).to_string(), *initial));
            }
            if balances.len() < 2 {
                return Err(WalletError::TooFewAccounts);
            }
            Ok(Wallet {
                balances,
                applied: Vec::new(),
                events: Vec::new(),
            })
        }

        pub fn balance(&self, account: &str) -> Option<u64> {
            self.balances
                .iter()
                .find(|(name, _)| name.as_str() == account)
                .map(|(_, value)| *value)
        }

        pub fn transfer(
            &mut self,
            transfer_id: &str,
            from: &str,
            to: &str,
            amount: u64,
        ) -> Result<TransferOutcome, WalletError> {
            validate_identifier(transfer_id)?;
            validate_identifier(from)?;
            validate_identifier(to)?;
            if from == to {
                return Err(WalletError::SameAccount);
            }
            if amount == 0 {
                return Err(WalletError::ZeroAmount);
            }

            let arguments_hash = event_mix(0, transfer_id, from, to, amount, 0);
            if let Some(previous) = self
                .applied
                .iter()
                .find(|(id, _)| id.as_str() == transfer_id)
                .map(|(_, hash)| *hash)
            {
                return if previous == arguments_hash {
                    Ok(TransferOutcome::Duplicate)
                } else {
                    Err(WalletError::TransferIdReuse)
                };
            }

            let source = self.balance(from).ok_or(WalletError::MissingAccount)?;
            let destination = self.balance(to).ok_or(WalletError::MissingAccount)?;
            let next_source = source.checked_sub(amount).ok_or(WalletError::Overdraft)?;
            let next_destination = destination
                .checked_add(amount)
                .ok_or(WalletError::DestinationOverflow)?;

            // Mutate only after every check and arithmetic operation has succeeded.
            let sequence = self.events.len() as u64;
            let previous_hash = self
                .events
                .last()
                .map(|event| event.hash)
                .unwrap_or(GENESIS_HASH);
            self.set_balance(from, next_source);
            self.set_balance(to, next_destination);
            self.applied.push((transfer_id.to_string(), arguments_hash));
            self.events.push(make_event(
                sequence,
                transfer_id,
                from,
                to,
                amount,
                previous_hash,
            ));
            Ok(TransferOutcome::Applied)
        }

        fn set_balance(&mut self, account: &str, value: u64) {
            let slot = self
                .balances
                .iter_mut()
                .find(|(name, _)| name.as_str() == account);
            if let Some(slot) = slot {
                slot.1 = value;
            } else {
                self.balances.push((account.to_string(), value));
            }
        }

        pub fn events(&self) -> &[CandidateEvent] {
            &self.events
        }

        pub fn tamper_event_hash(&mut self, index: usize) -> bool {
            match self.events.get_mut(index) {
                Some(event) => {
                    event.hash ^= 1;
                    true
                }
                None => false,
            }
        }

        pub fn verify_events(events: &[CandidateEvent]) -> bool {
            let mut previous_hash = GENESIS_HASH;
            let mut seen: Vec<&str> = Vec::new();
            for (index, event) in events.iter().enumerate() {
                if event.sequence != index as u64
                    || event.previous_hash != previous_hash
                    || seen.contains(&event.transfer_id.as_str())
                {
                    return false;
                }
                match recomputed_event_hash(events, index) {
                    Some(expected) if expected == event.hash => {}
                    _ => return false,
                }
                seen.push(event.transfer_id.as_str());
                previous_hash = event.hash;
            }
            true
        }
    }
}
