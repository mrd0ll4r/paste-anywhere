use std::hash::Hash;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::cmp::Ordering;
use std::fmt::Debug;

// This is the vector clock implementation taken from vectorclock-rs.
// Some things are cleaned up, some things were added, some removed.
// We have to copy it here to implement conflict resolution for concurrent clocks.

/// A TemporalRelation is a temporal relation between two VectorClocks.
/// Notably, Caused means "this clock caused the other clock", i.e. this clock came before the
/// other, and EffectOf means "this clock is an effect of the other clock", i.e. this clock came
/// after the other.
/// ConcurrentGreater and ConcurrentSmaller are relations necessary to resolve conflicts between
/// concurrent clocks.
#[derive(Debug, PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
pub enum TemporalRelation {
    Equal,
    Caused,
    EffectOf,
    ConcurrentGreater,
    ConcurrentSmaller,
}

/// A VectorClock implements the vector clock algorithm.
#[derive(PartialEq, Eq, Debug, Serialize, Deserialize, Clone)]
pub struct VectorClock<HostType: Hash + Eq + Clone + Ord + Debug> {
    entries: HashMap<HostType, u64>,
}

impl<HostType: Clone + Hash + Eq + Ord + Debug> VectorClock<HostType> {
    /// Creates an empty VectorClock.
    pub fn new() -> VectorClock<HostType> {
        VectorClock {
            entries: HashMap::new(),
        }
    }

    /// Increments the counter for [host] by one.
    pub fn incr(&mut self, host: HostType) {
        let count = self.entries.entry(host).or_insert(0);
        *count += 1
    }

    /// Makes a copy of the underlying state, increments the counter for [host] by one and returns
    /// the copy.
    pub fn incr_clone(&self, host: HostType) -> Self {
        let mut entries = self.entries.clone();
        {
            let mut count = entries.entry(host).or_insert(0);
            *count += 1;
        }
        VectorClock { entries }
    }

    /// Determines the temporal relation between this clock and [other].
    /// See the documentation on the `TemporalRelation` enum for an explanation of the values
    /// returned.
    pub fn temporal_relation(&self, other: &Self) -> TemporalRelation {
        if self == other {
            TemporalRelation::Equal
        } else if self.superseded_by(other) {
            // self caused other (smaller than other)
            TemporalRelation::Caused
        } else if other.superseded_by(self) {
            // self is an effect of other (larger than other)
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
        let mut own_keys: Vec<HostType> = Vec::new();
        let mut other_keys: Vec<HostType> = Vec::new();

        {
            for key in self.entries.keys() {
                own_keys.push(key.clone());
            }
            for key in other.entries.keys() {
                other_keys.push(key.clone());
            }
        }

        own_keys.as_mut_slice().sort();
        other_keys.as_mut_slice().sort();

        // Comparing vectors also compares their length, but only after comparing each element.
        // We move comparing the length before comparing the elements to avoid situations like:
        // - clock 1: [[B:1],[C:2],[D:1],[A:2]]
        // - clock 2: [[B:1],[C:2],[E:1]]
        // Using just vector comparison would order clock 1 < clock 2 (because the vectors are
        // sorted and A<B). Although these two are really concurrent and we can't enforce an order,
        // it looks like clock 1 should be ordered "greater than" clock 2, just because it captured
        // more events. This is arbitrary, one could decide not to follow this approach, but it felt
        // more natural to us.
        if own_keys.len() > other_keys.len() {
            return true;
        } else if other_keys.len() > own_keys.len() {
            return false;
        }

        let ord = own_keys.cmp(&other_keys);

        match ord {
            Ordering::Less => false,
            Ordering::Greater => true,
            Ordering::Equal => {
                // they have the same keys, just values are different.
                println!(
                    "clock: vector resolution failed: self clock: {:?}, other clock: {:?}",
                    self, other
                );

                for i in 0..own_keys.len() {
                    let own_val = self.entries.get(&own_keys[i]).unwrap();
                    let other_val = other.entries.get(&own_keys[i]).unwrap();
                    if own_val == other_val {
                        continue;
                    } else if own_val > other_val {
                        return true;
                    } else if own_val < other_val {
                        return false;
                    }
                }

                panic!("unable to resolve concurrent clocks");
            }
        }
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

    /// Merges a copy of this clock with [other], returning a new clock that is equal to or greater
    /// than both this and [other].
    pub fn merge_with(&self, other: &Self) -> Self {
        let mut entries = self.entries.clone();

        for (host, &other_n) in other.entries.iter() {
            let mut a = entries.entry(host.clone()).or_insert(other_n);
            if other_n > *a {
                *a = other_n;
            }
        }

        VectorClock { entries }
    }
}

#[cfg(test)]
mod test {
    use super::{TemporalRelation, VectorClock};

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

    #[test]
    fn test_diverged() {
        let base = StrVectorClock::new();
        let c1 = base.incr_clone("A");
        let c2 = base.incr_clone("B");

        assert!(!(c1 == c2));

        assert_eq!(
            c1.temporal_relation(&c2),
            TemporalRelation::ConcurrentSmaller
        );
        assert_eq!(
            c2.temporal_relation(&c1),
            TemporalRelation::ConcurrentGreater
        );
    }

    #[test]
    fn test_complex_concurrent() {
        let base = StrVectorClock::new();
        let mut c1 = base.incr_clone("A");
        let mut c2 = base.incr_clone("B");
        c1.incr("A");
        c1.incr("B");
        c2.incr("A");
        c2.incr("B");

        assert!(!(c1 == c2));

        assert_eq!(
            c1.temporal_relation(&c2),
            TemporalRelation::ConcurrentGreater
        );
        assert_eq!(
            c2.temporal_relation(&c1),
            TemporalRelation::ConcurrentSmaller
        );
    }

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
