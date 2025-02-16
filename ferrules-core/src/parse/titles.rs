#![allow(incomplete_features)]
use std::collections::HashMap;

use itertools::Itertools;
use kmeans::{EuclideanDistance, KMeans, KMeansConfig};

use crate::{
    blocks::TitleLevel,
    entities::{Element, ElementID, PageID},
};

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

    let kmean: KMeans<_, 4, _> = KMeans::new(samples, sample_len, 1, EuclideanDistance);

    let result = kmean.kmeans_lloyd(
        title_buckets,
        100,
        KMeans::init_kmeanplusplus,
        &KMeansConfig::default(),
    );

    // Note: first order the centroids by their height. Vectors of the centroid_id
    // Inverse to get a vectors of levels we can index in using the centroid assingment id for each element
    let centroid_lvl: Vec<u8> = result
        .centroids
        .iter()
        .enumerate()
        .sorted_by(|(_, c1), (_, c2)| c2[0].partial_cmp(&c1[0]).unwrap())
        .map(|(idx, _)| idx)
        .enumerate()
        .map(|(idx, val)| (val, idx))
        .sorted_by_key(|&(val, _)| val)
        .map(|(_, idx)| idx as u8)
        .collect();

    for (el, assignment) in titles.iter().zip(result.assignments.iter()) {
        title_level.insert((el.page_id, el.id), centroid_lvl[*assignment]);
    }

    title_level
}
