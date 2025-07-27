#!/bin/bash
echo "Starting a failing job..."
sleep 2
echo "Job failed with an error." >&2
exit 1
