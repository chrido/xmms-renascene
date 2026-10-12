//! Memory-only revision ledger shared by synchronous and asynchronous media
//! publication paths. Receivers must also validate the current source/owner.

use super::ArtworkError;

#[derive(Debug)]
pub struct ProjectionLedger<T> {
    revision: u64,
    latest: Option<T>,
}

impl<T> Default for ProjectionLedger<T> {
    fn default() -> Self {
        Self {
            revision: 0,
            latest: None,
        }
    }
}

impl<T: PartialEq> ProjectionLedger<T> {
    pub fn publish(&mut self, value: T) -> Result<u64, ArtworkError> {
        if self.latest.as_ref() != Some(&value) {
            let revision = self
                .revision
                .checked_add(1)
                .ok_or(ArtworkError::GenerationExhausted)?;
            self.latest = Some(value);
            self.revision = revision;
        }
        Ok(self.revision)
    }

    pub fn current(&self) -> Option<(u64, &T)> {
        self.latest.as_ref().map(|value| (self.revision, value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_state_reuses_revision_but_a_to_b_to_a_rejects_the_first_a() {
        let mut ledger = ProjectionLedger::default();
        let a = ledger.publish("a").unwrap();
        assert_eq!(ledger.publish("a"), Ok(a));
        let b = ledger.publish("b").unwrap();
        let new_a = ledger.publish("a").unwrap();
        assert!(a < b && b < new_a);
        assert_eq!(ledger.current(), Some((new_a, &"a")));
    }

    #[test]
    fn source_revision_change_invalidates_a_same_title_projection() {
        let mut ledger = ProjectionLedger::default();
        let first = ledger.publish(("same title", 1, "foreground")).unwrap();
        let replaced = ledger.publish(("same title", 2, "foreground")).unwrap();
        let background = ledger.publish(("same title", 2, "service")).unwrap();
        assert!(first < replaced && replaced < background);
    }

    #[test]
    fn exhausted_ledger_does_not_replace_accepted_state() {
        let mut ledger = ProjectionLedger {
            revision: u64::MAX,
            latest: Some("old"),
        };
        assert_eq!(
            ledger.publish("new"),
            Err(ArtworkError::GenerationExhausted)
        );
        assert_eq!(ledger.current(), Some((u64::MAX, &"old")));
    }
}
