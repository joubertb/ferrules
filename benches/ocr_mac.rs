#[allow(dead_code)]
#[cfg(target_os = "macos")]
mod test {

    use rand::Rng;

    use std::io::Cursor;

    use image::{DynamicImage, Rgba, RgbaImage};

    use objc2::ClassType;
    use objc2_foundation::{NSArray, NSData, NSDictionary};
    use objc2_vision::{
        VNImageRequestHandler, VNRecognizeTextRequest, VNRequest, VNSequenceRequestHandler,
    };

    use rayon::prelude::*;
    use std::{hint::black_box, time::Duration};

    use criterion::{criterion_main, Criterion};

    fn get_fake_images(count: usize, width: u32, height: u32) -> Vec<DynamicImage> {
        let mut rng = rand::thread_rng();
        let mut images = Vec::with_capacity(count);

        for _ in 0..count {
            let mut img = RgbaImage::new(width, height);
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

    fn run_ocr_imagehandler_par_iter(inputs: &[Cursor<Vec<u8>>]) {
        inputs.par_iter().for_each(|buffer| {
            unsafe {
                let request = VNRecognizeTextRequest::new();
                request.setRecognitionLevel(objc2_vision::VNRequestTextRecognitionLevel::Accurate);
                // TODO set the languages array
                request.setUsesLanguageCorrection(true);

                let handler = VNImageRequestHandler::initWithData_options(
                    VNImageRequestHandler::alloc(),
                    &NSData::with_bytes(buffer.get_ref()),
                    &NSDictionary::new(),
                );
                let requests = NSArray::from_slice(&[request.as_ref() as &VNRequest]);
                handler.performRequests_error(&requests).unwrap();
                request.results().unwrap();
            }
        });
    }

    fn run_ocr_imagehandler(inputs: &[Cursor<Vec<u8>>]) {
        inputs.iter().for_each(|buffer| {
            unsafe {
                let request = VNRecognizeTextRequest::new();
                request.setRecognitionLevel(objc2_vision::VNRequestTextRecognitionLevel::Accurate);
                // TODO set the languages array
                request.setUsesLanguageCorrection(true);

                let handler = VNImageRequestHandler::initWithData_options(
                    VNImageRequestHandler::alloc(),
                    &NSData::with_bytes(buffer.get_ref()),
                    &NSDictionary::new(),
                );
                let requests = NSArray::from_slice(&[request.as_ref() as &VNRequest]);
                handler.performRequests_error(&requests).unwrap();
                request.results().unwrap();
            }
        });
    }

    fn run_ocr_sequencehandler(inputs: &[Cursor<Vec<u8>>]) {
        unsafe {
            let handler = VNSequenceRequestHandler::new();
            inputs.iter().for_each(|buffer| {
                let request = VNRecognizeTextRequest::new();
                request.setRecognitionLevel(objc2_vision::VNRequestTextRecognitionLevel::Accurate);
                // TODO set the languages array
                request.setUsesLanguageCorrection(true);

                let requests = NSArray::from_slice(&[request.as_ref() as &VNRequest]);
                handler
                    .performRequests_onImageData_error(
                        &requests,
                        &NSData::with_bytes(buffer.get_ref()),
                    )
                    .unwrap();
                request.results().unwrap();
            });
        };
    }

    fn bench_ort_session(c: &mut Criterion) {
        // Setup inputs
        let mut group = c.benchmark_group("ocr_mac_bench");
        let n_batch = 20usize;
        let images = get_fake_images(n_batch, 1024, 1024);

        let inputs: Vec<Cursor<Vec<u8>>> = images
            .into_iter()
            .map(|image| {
                let mut buffer: Cursor<Vec<u8>> = Cursor::new(Vec::new());
                image
                    .write_to(&mut buffer, image::ImageFormat::Png)
                    .unwrap();
                buffer
            })
            .collect::<Vec<_>>();

        group.bench_function("run_ocr_imagehandler", |b| {
            b.iter(|| run_ocr_imagehandler(black_box(&inputs)))
        });

        group.bench_function("run_ocr_imagehandler_par_iter", |b| {
            b.iter(|| run_ocr_imagehandler_par_iter(black_box(&inputs)))
        });
        group.bench_function("run_ocr_sequencehandler", |b| {
            b.iter(|| run_ocr_sequencehandler(black_box(&inputs)))
        });
        group.finish();
    }

    criterion::criterion_group! {
        name = benches;
        config = Criterion::default().measurement_time(Duration::from_secs(10));
        targets = bench_ort_session
    }

    criterion_main!(benches);
}
