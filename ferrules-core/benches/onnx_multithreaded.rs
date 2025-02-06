use ndarray::Array4;
use ort::{inputs, session::Session};

use rayon::prelude::*;
use std::{hint::black_box, time::Duration};

use criterion::{criterion_main, Criterion};
use ferrules_core::layout::model::LAYOUT_MODEL_BYTES;

fn run_iter(session: &Session, input: &[Array4<f32>]) {
    input.iter().for_each(|i| {
        session.run(inputs!["images"=>i.view()].unwrap()).unwrap();
    });
}

fn run_par_iter(session: &Session, input: &[Array4<f32>]) {
    input.par_iter().for_each(|i| {
        session.run(inputs!["images"=>i.view()].unwrap()).unwrap();
    });
}

fn bench_ort_session(c: &mut Criterion) {
    // Setup inputs
    let mut group = c.benchmark_group("ort_bench");
    let session = Session::builder()
        .unwrap()
        .with_execution_providers(
            [ort::execution_providers::CoreMLExecutionProvider::default()
                // .with_ane_only()
                .with_subgraphs()
                .build()],
        )
        .unwrap()
        .commit_from_memory(LAYOUT_MODEL_BYTES)
        .unwrap();

    let n_batch = 20usize;

    let input: Vec<Array4<f32>> = (0..n_batch)
        .map(|_| Array4::ones([1, 3, 1024, 1024]))
        .collect();
    session
        .run(inputs!["images"=>input[0].view()].unwrap())
        .unwrap();

    group.bench_function("run_iter", |b| {
        b.iter(|| run_iter(black_box(&session), black_box(&input)))
    });

    group.bench_function("run_par_iter", |b| {
        b.iter(|| run_par_iter(black_box(&session), black_box(&input)))
    });
    group.finish();
}

criterion::criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(10));
    targets = bench_ort_session
}

criterion_main!(benches);
