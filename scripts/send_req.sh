#!/bin/bash

curl -v -X POST http://localhost:3002/parse \
  -H "Accept: text/markdown" \
  -H "Content-Type: multipart/form-data" \
  -F "file=@$1"
# -F 'options={"page_range": "1-5"}'
