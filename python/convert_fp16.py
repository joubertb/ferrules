import onnx
from onnxconverter_common import float16

model = onnx.load("./models/yolov8s-doclaynet.onnx")
model_fp16 = float16.convert_float_to_float16(model)
onnx.save(model_fp16, "./models/yolov8s-doclaynet-fp16.onnx")
