from ultralytics import YOLO

# Load the YOLO11 model
# model = YOLO("yolov8s-doclaynet.pt")

# Export the model to ONNX format
# model.export(format="onnx")

# Load the exported ONNX model
onnx_model = YOLO("../models/yolov8s-doclaynet.onnx")
result = onnx_model("slide.jpg")

height = result.orig_shape[0]
width = result.orig_shape[1]
label_boxes = []
for label, box in zip(result.boxes.cls.tolist(), result.boxes.xyxyn.tolist()):
    label_boxes.append(
        (
            result.names[int(label)],
            [box[0] * width, box[1] * height, box[2] * width, box[3] * height],
        )
    )
print(
    f"Detected objects: {len(label_boxes)}, Image size: {width}x{height}, Speed: {result.speed}"
)
# # Run inference
