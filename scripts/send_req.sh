#!/bin/bash

curl -v -X POST http://localhost:3002/parse \
  -H "Content-Type: multipart/form-data" \
  -F "file=@$1"
# -F 'options={"page_range": "1-5"}'
