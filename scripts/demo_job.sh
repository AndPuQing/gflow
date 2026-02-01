#!/bin/bash
# Demo job for gflow showcase

echo "=========================================="
echo "gflow Demo Job"
echo "=========================================="
echo ""
echo "Job ID: $GFLOW_JOB_ID"
echo "Started at: $(date)"
echo "Running on GPU: $CUDA_VISIBLE_DEVICES"
echo ""

# Simulate some GPU work
echo "Checking GPU availability..."
if command -v nvidia-smi &> /dev/null; then
    nvidia-smi --query-gpu=index,name,memory.total,memory.free --format=csv,noheader
else
    echo "nvidia-smi not available (demo mode)"
fi

echo ""
echo "Simulating computation..."
for i in {1..3}; do
    echo "Progress: $((i*33))%"
    sleep 1
done
echo "Progress: 100%"

echo ""
echo "Job completed at: $(date)"
echo "=========================================="
