#!/bin/bash
# Actual demo script for recording with real gflow commands

set -e

# Clean up gflow data directory before recording
echo "Cleaning up gflow data directory..."
rm -rf ~/.local/share/gflow
sleep 1

# Ensure proper encoding
export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8
export TERM=xterm-256color

# Helper functions
comment() {
    echo ""
    echo "# $1"
    sleep 1.5
}

run_cmd() {
    echo "\$ $1"
    sleep 0.8
    eval "$1"
    sleep 1.5
}

# Main demo
clear
echo "=========================================="
echo "  gflow - GPU Job Scheduler Demo"
echo "=========================================="
sleep 2

comment "Step 1: Start the scheduler daemon"
run_cmd "gflowd up"

comment "Step 2: Check GPU resources"
run_cmd "ginfo"

comment "Step 3: Submit a job requesting 1 GPU"
run_cmd "gbatch --gpus 1 --name demo-job-1 scripts/demo_job.sh"

comment "Step 4: Check the job queue"
run_cmd "gqueue"

comment "Step 5: Submit more jobs to demonstrate queuing"
run_cmd "gbatch --gpus 1 --name demo-job-2 scripts/demo_job.sh"
run_cmd "gbatch --gpus 1 --name demo-job-3 scripts/demo_job.sh"

comment "Step 6: View the queue with multiple jobs"
run_cmd "gqueue"

comment "Step 7: Wait a moment and check queue again"
sleep 2
run_cmd "gqueue"

comment "Step 8: View job logs (first completed job)"
run_cmd "gjob log 1"

comment "Step 9: Cancel a pending job (if any)"
run_cmd "gcancel 3 2>/dev/null || echo 'Job already completed or not found'"
run_cmd "gqueue"

comment "Step 10: Stop the scheduler"
run_cmd "gflowd down"

echo ""
echo "=========================================="
echo "  Demo Complete!"
echo "  Documentation: https://runqd.com"
echo "=========================================="
sleep 3
