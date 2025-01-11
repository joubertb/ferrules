use rand::Rng;

use std::{hint::black_box, iter::repeat, time::Duration};

use criterion::{criterion_main, Criterion};
use ferrules::layout::model::ORTLayoutParser;
use image::{DynamicImage, Rgba, RgbaImage};

fn get_fake_images(count: usize, width: u32, height: u32) -> Vec<DynamicImage> {
    let mut rng = rand::thread_rng();
    let mut images = Vec::with_capacity(ORTLayoutParser::BATCH_SIZE);

    for _ in 0..count {
        // Create an empty RgbaImage
        let mut img = RgbaImage::new(width, height);

        // Fill the image with random pixel values
        for pixel in img.pixels_mut() {
            *pixel = Rgba([
                rng.gen_range(0..=255), // Random red
                rng.gen_range(0..=255), // Random green
                rng.gen_range(0..=255), // Random blue
                255,                    // Fully opaque
            ]);
        }
        images.push(DynamicImage::ImageRgba8(img));
    }

    images
}

fn parse_loop(model: &ORTLayoutParser, images: &[DynamicImage], factors: &[f32]) {
    for (image, factor) in images.iter().zip(factors.iter()) {
        let input = model.parse_layout(image, *factor);
        let _bbox = input.unwrap();
    }
}

fn parse_batch(model: &ORTLayoutParser, images: &[DynamicImage], factors: &[f32]) {
    let result = model.parse_layout_batch(images, factors);
    result.unwrap()
}

fn bench_layout(c: &mut Criterion) {
    // Setup inputs
    let layout_model_single_batch =
        ORTLayoutParser::new("./models/yolov8s-doclaynet.onnx").expect("can't load layout model");

    let layout_model_batch = ORTLayoutParser::new("./models/yolov8s-doclaynet-batch-16.onnx")
        .expect("can't load layout model");
    let number_images = ORTLayoutParser::BATCH_SIZE;
    let images = get_fake_images(
        number_images,
        ORTLayoutParser::REQUIRED_WIDTH,
        ORTLayoutParser::REQUIRED_HEIGHT,
    );

    let rescale_factors: Vec<f32> = repeat(1f32).take(number_images).collect();

    // Group
    let mut group = c.benchmark_group("Layout_Parsing");
    group.sample_size(10);
    group.bench_function("run_batch_16", |b| {
        b.iter(|| {
            parse_batch(
                black_box(&layout_model_batch),
                black_box(&images),
                black_box(&rescale_factors),
            )
        })
    });

    group.bench_function("run_batch_1", |b| {
        b.iter(|| {
            parse_loop(
                black_box(&layout_model_single_batch),
                black_box(&images),
                black_box(&rescale_factors),
            )
        })
    });

    group.finish();
}

criterion::criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(10));
    targets = bench_layout
}

criterion_main!(benches);
