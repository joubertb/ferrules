from ultralytics import YOLO

# Load the YOLO11 model
model = YOLO("yolov8s-doclaynet.pt")


# Export the model to ONNX format
model.export(format="onnx", simplify=True)
