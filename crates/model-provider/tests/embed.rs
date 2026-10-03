//! Batch planning and reply checks for embeddings.

use model_provider::{
    BatchMax, Count, Dims, EmbedEnd, EmbedFault, EmbedVector, ModelName, TurnUsage, plan_batches,
};
use proptest::prelude::*;

fn end(widths: &[usize]) -> EmbedEnd {
    EmbedEnd {
        vectors: widths.iter().map(|w| EmbedVector(vec![0.5; *w])).collect(),
        usage: TurnUsage::default(),
        served: ModelName("m".into()),
    }
}

#[test]
fn batches() {
    type Case = (usize, u32, &'static [(usize, usize)]);
    const CASES: &[Case] = &[
        (0, 4, &[]),
        (1, 4, &[(0, 1)]),
        (4, 4, &[(0, 4)]),
        (5, 4, &[(0, 4), (4, 5)]),
        (9, 4, &[(0, 4), (4, 8), (8, 9)]),
        (3, 0, &[(0, 1), (1, 2), (2, 3)]),
        (3, 1, &[(0, 1), (1, 2), (2, 3)]),
        (3, u32::MAX, &[(0, 3)]),
    ];
    for (n, max, want) in CASES {
        let got: Vec<(usize, usize)> = plan_batches(*n, BatchMax(*max))
            .into_iter()
            .map(|r| (r.start, r.end))
            .collect();
        assert_eq!(&got, want, "n={n} max={max}");
    }
}

proptest! {
    #[test]
    fn batches_cover_the_inputs_exactly_once_in_order(n in 0usize..5000, max in 0u32..70) {
        let ranges = plan_batches(n, BatchMax(max));
        let step = max.max(1) as usize;
        let mut next = 0;
        for range in &ranges {
            prop_assert_eq!(range.start, next);
            prop_assert!(range.end > range.start && range.end - range.start <= step);
            next = range.end;
        }
        prop_assert_eq!(next, n);
        prop_assert_eq!(plan_batches(n, BatchMax(max)), ranges);
    }
}

#[test]
fn a_reply_is_checked_for_count_then_width() {
    assert_eq!(end(&[3, 3]).check(Count(2), Dims(3)), Ok(()));
    assert_eq!(end(&[]).check(Count(0), Dims(3)), Ok(()));
    assert_eq!(
        end(&[3]).check(Count(2), Dims(3)),
        Err(EmbedFault::CountMismatch {
            want: Count(2),
            got: Count(1)
        })
    );
    assert_eq!(
        end(&[3, 2, 4]).check(Count(3), Dims(3)),
        Err(EmbedFault::WidthMismatch {
            want: Dims(3),
            got: Dims(2)
        })
    );
    // The count is checked before any width.
    assert_eq!(
        end(&[9]).check(Count(2), Dims(3)),
        Err(EmbedFault::CountMismatch {
            want: Count(2),
            got: Count(1)
        })
    );
}
