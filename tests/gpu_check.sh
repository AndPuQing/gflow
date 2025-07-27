#!/bin/bash
echo "Checking for allocated GPUs..."
if [ -n "$CUDA_VISIBLE_DEVICES" ]; then
  echo "CUDA_VISIBLE_DEVICES is set to: $CUDA_VISIBLE_DEVICES"
  exit 0
else
  echo "Error: CUDA_VISIBLE_DEVICES is not set." >&2
  exit 1
fi
