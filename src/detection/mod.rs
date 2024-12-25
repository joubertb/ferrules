pub mod model;

use candle_core::{DType, Device};
use candle_nn::VarBuilder;
use model::{Multiples, YoloV8};

#[test]
pub fn infer() -> anyhow::Result<()> {
    let model = "./models/yolov8s-doclaynet.safetensors";
    // let device = Device::new_metal(0)?;
    let device = Device::Cpu;
    let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[model], DType::F32, &device)? };
    let num_classes = 11;
    let model = YoloV8::load(vb, Multiples::s(), num_classes)?;
    dbg!(model);
    Ok(())
}
