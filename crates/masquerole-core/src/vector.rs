//! Vector math and a small in-memory index for finding the stored texts closest in meaning
//! to a question.

/// Scale `v` to length one. A zero vector stays zero.
pub fn normalize(v: &mut [f32]) {
    let length = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if length > 0.0 {
        v.iter_mut().for_each(|x| *x /= length);
    }
}

/// How alike two vectors point, from -1 to 1. Zero when the lengths differ or one is zero.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let (la, lb) = (
        a.iter().map(|x| x * x).sum::<f32>().sqrt(),
        b.iter().map(|x| x * x).sum::<f32>().sqrt(),
    );
    if la == 0.0 || lb == 0.0 {
        0.0
    } else {
        dot / (la * lb)
    }
}

/// Pack a vector as little-endian bytes, for storing in the database.
pub fn to_bytes(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

/// Unpack bytes made by [`to_bytes`]. A trailing partial number is dropped.
pub fn from_bytes(bytes: &[u8]) -> Vec<f32> {
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| f32::from_le_bytes(*c))
        .collect()
}

/// The ids of the `k` vectors closest to `query` that score at least `min_score`, best first.
pub fn nearest<I: Copy>(
    query: &[f32],
    candidates: &[(I, Vec<f32>)],
    k: usize,
    min_score: f32,
) -> Vec<(I, f32)> {
    let mut scored: Vec<(I, f32)> = candidates
        .iter()
        .map(|(id, v)| (*id, cosine(query, v)))
        .filter(|(_, score)| *score >= min_score)
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored.truncate(k);
    scored
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_of_equal_orthogonal_and_opposite_vectors() {
        assert!((cosine(&[1.0, 0.0], &[2.0, 0.0]) - 1.0).abs() < 1e-6);
        assert_eq!(cosine(&[1.0, 0.0], &[0.0, 1.0]), 0.0);
        assert!((cosine(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 0.0]), 0.0);
        assert_eq!(cosine(&[1.0], &[1.0, 0.0]), 0.0, "different lengths");
    }

    #[test]
    fn normalizing_gives_length_one_and_keeps_zero() {
        let mut v = vec![3.0, 4.0];
        normalize(&mut v);
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
        let mut zero = vec![0.0, 0.0];
        normalize(&mut zero);
        assert_eq!(zero, [0.0, 0.0]);
    }

    #[test]
    fn bytes_round_trip() {
        let v = vec![0.5, -1.25, 3.0];
        assert_eq!(from_bytes(&to_bytes(&v)), v);
        assert_eq!(from_bytes(&[1, 2, 3, 4, 5]).len(), 1);
    }

    #[test]
    fn nearest_orders_by_score_and_applies_the_limits() {
        let items = vec![
            (1, vec![1.0, 0.0]),
            (2, vec![0.9, 0.1]),
            (3, vec![0.0, 1.0]),
        ];
        let hits = nearest(&[1.0, 0.0], &items, 2, 0.0);
        assert_eq!(hits.iter().map(|h| h.0).collect::<Vec<_>>(), [1, 2]);
        let strict = nearest(&[1.0, 0.0], &items, 5, 0.5);
        assert_eq!(strict.len(), 2, "the orthogonal one is below the minimum");
        assert!(nearest::<i32>(&[1.0], &[], 3, 0.0).is_empty());
    }
}
