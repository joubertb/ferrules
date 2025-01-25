use ndarray::Array4;
use ort::{inputs, session::Session};

use ort::execution_providers::{CUDAExecutionProvider, TensorRTExecutionProvider};

use rayon::prelude::*;
use std::{hint::black_box, time::Duration};

use criterion::{criterion_main, Criterion};

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

fn run_batch(session: &Session, input: &Array4<f32>) {
    session
        .run(inputs!["images"=>input.view()].unwrap())
        .unwrap();
}

fn bench_ort_session(c: &mut Criterion) {
    // Setup inputs
    let mut group = c.benchmark_group("ort_bench");
    let single_session = Session::builder()
        .unwrap()
        .with_execution_providers([
            TensorRTExecutionProvider::default().build(),
            CUDAExecutionProvider::default().build(),
        ])
        .unwrap()
        .commit_from_file("models/yolov8s-doclaynet.onnx")
        .unwrap();

    let batch_session = Session::builder()
        .unwrap()
        .with_execution_providers([
            TensorRTExecutionProvider::default().build(),
            CUDAExecutionProvider::default().build(),
        ])
        .unwrap()
        .commit_from_file("models/yolov8s-doclaynet-batch-16.onnx")
        .unwrap();

    let n_batch = 16usize;

    // Warmap
    let vec_input: Vec<Array4<f32>> = (0..n_batch)
        .map(|_| Array4::ones([1, 3, 1024, 1024]))
        .collect();

    let batch_input: Array4<f32> = Array4::ones([16, 3, 1024, 1024]);

    run_iter(&single_session, &vec_input);
    group.bench_function("run_iter", |b| {
        b.iter(|| run_iter(black_box(&single_session), black_box(&vec_input)))
    });

    group.bench_function("run_par_iter", |b| {
        b.iter(|| run_par_iter(black_box(&single_session), black_box(&vec_input)))
    });

    run_batch(&batch_session, &batch_input);
    group.bench_function("run_batch", |b| {
        b.iter(|| run_batch(black_box(&batch_session), black_box(&batch_input)))
    });
    group.finish();
}

criterion::criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(10));
    targets = bench_ort_session
}

criterion_main!(benches);
