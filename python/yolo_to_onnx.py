import torch
from ultralytics import YOLO

# Load the YOLO11 model

model = YOLO("yolov10s-doclaynet.pt")

input = torch.randn(1, 3, 1024, 1024)

model.eval()
res = model(input)[0]

# Export the model to ONNX format
# model.export(format="onnx")
