# Ferrules API Documentation

## Overview

Ferrules API provides a HTTP interface to the document parsing capabilities of Ferrules. The API server is available through the `ferrules-api` binary.

## Server Configuration

The API server can be configured using the following options:

```sh
Options:
  --otlp-endpoint <OTLP_ENDPOINT>        OpenTelemetry collector endpoint [default: http://localhost:4317]
  --sentry-dsn <SENTRY_DSN>             Sentry DSN for error tracking
  --sentry-environment <SENTRY_ENVIRONMENT>  Sentry environment [default: dev]
  --listen-addr <LISTEN_ADDR>           API listen address [default: 0.0.0.0:3002]
  --sentry-debug                        Enable debug mode for Sentry
  --coreml                             Enable CoreML for layout inference
  --use-ane                            Enable Apple Neural Engine acceleration
  --trt                                Enable TensorRT for layout inference
  --cuda                               Enable CUDA for layout inference
  --device-id <DEVICE_ID>              CUDA device ID [default: 0]
  -j, --intra-threads <INTRA_THREADS>  Threads for parallel processing [default: 16]
  --inter-threads <INTER_THREADS>      Threads for parallel operations [default: 4]
  -O, --graph-opt-level <LEVEL>        Ort graph optimization level
```

## Environment Variables

The following environment variables can be used to configure the API server:

- `OTLP_ENDPOINT`: OpenTelemetry collector endpoint
- `SENTRY_DSN`: Sentry DSN for error tracking
- `SENTRY_ENVIRONMENT`: Sentry environment
- `API_LISTEN_ADDR`: API listen address
- `SENTRY_DEBUG`: Enable Sentry debug mode

## Performance Tuning

### Hardware Acceleration

- Use `--coreml` and `--use-ane` for Apple Silicon devices
- Use `--cuda` and `--device-id` for NVIDIA GPUs
- Use `--trt` for TensorRT acceleration

### Threading

- `--intra-threads`: Controls parallel processing within operations
- `--inter-threads`: Controls parallel execution of operations

## API Endpoints

[API endpoints documentation to be added]

## Examples

[Usage examples to be added]

## Monitoring

The API server supports:

1. OpenTelemetry for metrics and tracing
2. Sentry for error tracking

Configure the endpoints using the appropriate flags or environment variables.
