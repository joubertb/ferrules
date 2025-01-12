from time import perf_counter
import numpy as np
import onnxruntime as ort


# Initialize ONNX Runtime session with CoreML Execution Provider
def create_session_with_coreml(model_path):
    # Create session options
    session_options = ort.SessionOptions()
    session_options.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL

    # Use CoreMLExecutionProvider with settings for ANE only
    # providers = ["CPUExecutionProvider"]
    providers =['CUDAExecutionProvider']
    # providers = [("CoreMLExecutionProvider", {"use_ane": True})]

    session = ort.InferenceSession(
        model_path, sess_options=session_options, providers=providers
    )
    return session


def run_inference(session, input_tensor):
    # Get model input name
    input_name = session.get_inputs()[0].name
    # Run inference
    output = session.run(None, {input_name: input_tensor})
    return output


if __name__ == "__main__":

    ##### SINGLE
    onnx_model_path = "./models/yolov8s-doclaynet.onnx"
    single_batch_session = create_session_with_coreml(onnx_model_path)
    input_tensor = np.random.rand(1, 3, 1024, 1024).astype(np.float32)
    _ = run_inference(single_batch_session, input_tensor)
    s = perf_counter()
    for i in range(100):
        outputs = run_inference(single_batch_session, input_tensor)
    e = perf_counter()
    print(f"Model {onnx_model_path} took: {e-s:.2f}s")

    ##### SINGLE
    onnx_model_path = "./models/yolov8s-doclaynet-fp16.onnx"
    single_batch_session = create_session_with_coreml(onnx_model_path)
    input_tensor = np.random.rand(1, 3, 1024, 1024).astype(np.float16)
    _ = run_inference(single_batch_session, input_tensor)
    s = perf_counter()
    for i in range(100):
        outputs = run_inference(single_batch_session, input_tensor)
    e = perf_counter()
    print(f"Model {onnx_model_path} took: {e-s:.2f}s")
