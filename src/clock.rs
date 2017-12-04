use std::hash::Hash;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

// This is the taken from vectorclock-rs.
// Some things are cleaned up, some things were added, some removed.
// We have to copy it here to implement Serialize and Deserialize.

#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum TemporalRelation {
    Equal,
    Caused,
    EffectOf,
    ConcurrentGreater,
    ConcurrentSmaller,
}

#[derive(PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VectorClock<HostType: Hash + Eq> {
    entries: HashMap<HostType, u64>,
}

impl<HostType: Clone + Hash + Eq> VectorClock<HostType> {
    pub fn new() -> VectorClock<HostType> {
        VectorClock {
            entries: HashMap::new(),
        }
    }

    pub fn incr(&mut self, host: HostType) {
        let count = self.entries.entry(host).or_insert(0);
        *count += 1
    }

    /// incr_clone clones the clock and increments the given host's value by one.
    /// this is the original implementation of increment, but I think it sucks.
    pub fn incr_clone(&self, host: HostType) -> Self {
        let mut entries = self.entries.clone();

        match entries.entry(host) {
            Entry::Vacant(e) => { e.insert(1); }
            Entry::Occupied(mut e) => {
                let v = *e.get();
                e.insert(v + 1);
            }
        };

        VectorClock {
            entries,
        }
    }

    pub fn temporal_relation(&self, other: &Self) -> TemporalRelation {
        if self == other {
            TemporalRelation::Equal
        } else if self.superseded_by(other) {
            // self caused other
            TemporalRelation::Caused
        } else if other.superseded_by(self) {
            // self is an effect of other
            TemporalRelation::EffectOf
        } else {
            if self.is_greater_concurrent_than(other) {
                // self and other are concurrent, but we can order self to be greater than other
                TemporalRelation::ConcurrentGreater
            } else {
                // self and other are concurrent, but we can order self to be smaller than other
                TemporalRelation::ConcurrentSmaller
            }
        }
    }

    fn is_greater_concurrent_than(&self, other: &Self) -> bool {
        // TODO figure something out - maybe just go through the clocks in order?
        false
    }

    fn superseded_by(&self, other: &Self) -> bool {
        let mut has_smaller = false;

        for (host, &self_n) in self.entries.iter() {
            let other_n = *other.entries.get(host).unwrap_or(&0);

            if self_n > other_n {
                return false;
            }

            has_smaller = has_smaller || (self_n < other_n);
        }

        for (host, &other_n) in other.entries.iter() {
            let self_n = *self.entries.get(host).unwrap_or(&0);

            if self_n > other_n {
                return false;
            }

            has_smaller = has_smaller || (self_n < other_n);
        }

        has_smaller
    }

    pub fn merge_with(&self, other: &Self) -> Self {
        let mut entries = self.entries.clone();

        for (host, &other_n) in other.entries.iter() {
            // TODO clean this up
            match entries.entry(host.clone()) {
                Entry::Vacant(e) => { e.insert(other_n); }
                Entry::Occupied(mut e) => {
                    let self_n = *e.get();

                    if other_n > self_n {
                        e.insert(other_n);
                    }
                }
            }
        }

        VectorClock {
            entries,
        }
    }
}

#[cfg(test)]
mod test {
    use super::{VectorClock, TemporalRelation};

    type StrVectorClock = VectorClock<&'static str>;

    #[test]
    fn test_empty_ordering() {
        let c1 = StrVectorClock::new();
        let c2 = StrVectorClock::new();

        assert_eq!(c1, c2);

        assert_eq!(c1.temporal_relation(&c2), TemporalRelation::Equal);
        assert_eq!(c2.temporal_relation(&c1), TemporalRelation::Equal);
    }

    #[test]
    fn test_incremented_ordering() {
        let c1 = StrVectorClock::new();
        let c2 = c1.incr_clone("A");

        assert!(!(c1 == c2));

        assert_eq!(c1.temporal_relation(&c2), TemporalRelation::Caused);
        assert_eq!(c2.temporal_relation(&c1), TemporalRelation::EffectOf);
    }


    /*
    #[test]
    fn test_diverged() {
        let base = StrVectorClock::new();
        let c1 = base.incr_clone("A");
        let c2 = base.incr_clone("B");

        assert!(!(c1 == c2));

        assert_eq!(c1.temporal_relation(&c2), TemporalRelation::Concurrent);
        assert_eq!(c2.temporal_relation(&c1), TemporalRelation::Concurrent);
    }*/

    #[test]
    fn test_merged() {
        let base = StrVectorClock::new();
        let c1 = base.incr_clone("A");
        let c2 = base.incr_clone("B");

        let m = c1.merge_with(&c2);

        assert_eq!(m.temporal_relation(&c1), TemporalRelation::EffectOf);
        assert_eq!(c1.temporal_relation(&m), TemporalRelation::Caused);

        assert_eq!(m.temporal_relation(&c2), TemporalRelation::EffectOf);
        assert_eq!(c2.temporal_relation(&m), TemporalRelation::Caused);
    }
}