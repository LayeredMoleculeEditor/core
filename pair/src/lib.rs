use serde::{Deserialize, Serialize};

#[derive(
    Debug, Default, Deserialize, Serialize, Hash, PartialEq, Eq, PartialOrd, Ord, Clone, Copy,
)]
pub struct Pair<T>(T, T);

impl<T> Pair<T> {
    pub fn new(a: T, b: T) -> Self {
        Self(a, b)
    }
}

impl<T: PartialOrd> Pair<T> {
    pub fn new_ordered(a: T, b: T) -> Self {
        if a >= b {
            Self::new(a, b)
        } else {
            Self::new(b, a)
        }
    }
}

impl<T: PartialEq> Pair<T> {
    pub fn another(&self, target: &T) -> Option<&T> {
        let Self(a, b) = self;
        if a == target {
            Some(b)
        } else if b == target {
            Some(a)
        } else {
            None
        }
    }

    pub fn contains(&self, target: &T) -> bool {
        self.another(target).is_some()
    }
}

mod test {
    #[test]
    fn ordered_pair() {
        use crate::Pair;
        assert_eq!(Pair::new_ordered(1, 2), Pair::new_ordered(2, 1));
    }

    #[test]
    fn pair_as_hash_key() {
        use crate::Pair;
        use std::collections::HashMap;

        assert_eq!(
            HashMap::from([
                (Pair::new_ordered(1, 2), 1),
                (Pair::new_ordered(2, 1), 2),
                (Pair::new_ordered(3, 4), 3)
            ]),
            HashMap::from([(Pair::new_ordered(1, 2), 2), (Pair::new_ordered(3, 4), 3)])
        )
    }

    #[test]
    fn pair_get_another_order() {
        use crate::Pair;
        use std::collections::HashSet;

        let pairs = HashSet::from([
            Pair::new_ordered(1, 2),
            Pair::new_ordered(2, 1),
            Pair::new_ordered(1, 3),
            Pair::new_ordered(3, 2),
        ]);

        for i in 1..=3 {
            assert_eq!(
                pairs
                    .iter()
                    .filter(|pair| pair.contains(&i))
                    .collect::<Vec<_>>()
                    .len(),
                2
            );
        }
    }
}
