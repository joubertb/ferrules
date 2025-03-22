#![allow(incomplete_features)]
use std::collections::HashMap;

use itertools::Itertools;
use kmeans::{EuclideanDistance, KMeans, KMeansConfig};

use crate::{
    blocks::TitleLevel,
    entities::{Element, ElementID, PageID},
};
/// Minimum gap between headings to consider them in separate buckets
const TITLE_MERGE_THRESHOLD: f32 = 0.7;
const LANE_COUNT_SIMD_KMEANS: usize = 4;

pub fn title_levels_kmeans(
    titles: &[&Element],
    title_buckets: usize,
) -> HashMap<(PageID, ElementID), TitleLevel> {
    let mut title_level = HashMap::new();

    let samples: Vec<f32> = titles.iter().map(|e| e.bbox.height()).collect();
    let sample_len = samples.len();

    // TODO: Check this heuristic
    if sample_len <= title_buckets {
        return title_level;
    }

    let kmean: KMeans<_, LANE_COUNT_SIMD_KMEANS, _> =
        KMeans::new(samples, sample_len, 1, EuclideanDistance);

    let result = kmean.kmeans_lloyd(
        title_buckets,
        100,
        KMeans::init_kmeanplusplus,
        &KMeansConfig::default(),
    );

    let centroids_sorted: Vec<_> = result
        .centroids
        .iter()
        .enumerate()
        .map(|(c_idx, c)| (c_idx, c[0]))
        .sorted_by(|(_, c1), (_, c2)| c2.total_cmp(c1))
        .collect();

    let mut centroid_mapping = vec![-1i8; centroids_sorted.len()];

    let mut prev_centroid = (1, centroids_sorted[0].1);
    for (c_idx, c_val) in centroids_sorted.iter() {
        if *c_val < prev_centroid.1 * TITLE_MERGE_THRESHOLD {
            prev_centroid.0 += 1;
        }
        centroid_mapping[*c_idx] = prev_centroid.0;
        prev_centroid.1 = *c_val;
    }

    for (el, assignment) in titles.iter().zip(result.assignments.iter()) {
        assert!(centroid_mapping[*assignment] >= 0);
        title_level.insert((el.page_id, el.id), centroid_mapping[*assignment] as u8);
    }

    title_level
}
