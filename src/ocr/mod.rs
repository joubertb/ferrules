use std::io::Cursor;

use image::DynamicImage;
use objc2::ClassType;
use objc2_foundation::{NSArray, NSData, NSDictionary};
use objc2_vision::{VNImageRequestHandler, VNRecognizeTextRequest, VNRequest};

const CONFIDENCE_THRESHOLD: f32 = 0f32;

pub fn parse_image_ocr(image: &DynamicImage) -> anyhow::Result<()> {
    unsafe {
        let req = VNRecognizeTextRequest::new();
        req.setRecognitionLevel(objc2_vision::VNRequestTextRecognitionLevel::Fast);

        let mut buffer: Cursor<Vec<u8>> = Cursor::new(Vec::new());
        image.write_to(&mut buffer, image::ImageFormat::Png)?;

        let handler = VNImageRequestHandler::initWithData_options(
            VNImageRequestHandler::alloc(),
            &NSData::with_bytes(buffer.get_ref()),
            &NSDictionary::new(),
        );
        let requests = NSArray::from_slice(&[req.as_ref() as &VNRequest]);
        handler.performRequests_error(&requests)?;

        if let Some(result) = req.results() {
            for obs in result.to_vec() {
                if (*obs).confidence() > CONFIDENCE_THRESHOLD {
                    let candidate = obs.topCandidates(1);
                    for rec in candidate.to_vec().into_iter() {
                        dbg!(rec.confidence());
                        dbg!(rec.string());
                    }
                }
            }
        }
    }
    Ok(())
}

mod tests {
    use std::time::Instant;

    use super::*;
    use image::ImageReader;

    #[test]
    fn test_apple_vision() {
        let image = ImageReader::open("./test_data/slide.jpg")
            .unwrap()
            .decode()
            .unwrap();

        let s = Instant::now();
        assert!(parse_image_ocr(&image).is_ok());
        println!(
            "OCR took: {}ms",
            Instant::now().duration_since(s).as_millis()
        );
    }
}
