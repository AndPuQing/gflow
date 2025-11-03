#!/bin/bash
#
# =========================================  gflow  =========================================
#  ██████╗ ███████╗██╗      ██████╗ ██╗    ██╗
# ██╔════╝ ██╔════╝██║     ██╔═══██╗██║    ██║
# ██║  ███╗█████╗  ██║     ██║   ██║██║ █╗ ██║
# ██║   ██║██╔══╝  ██║     ██║   ██║██║███╗██║
# ╚██████╔╝██║     ███████╗╚██████╔╝╚███╔███╔╝
#  ╚═════╝ ╚═╝     ╚══════╝ ╚═════╝  ╚══╝╚══╝
#
# A lightweight, single-node GPU job scheduler
# ==========================================================================================
#
# Job Configuration
# -----------------
# Use the GFLOW directives below to configure your job.
# These settings can be overridden by command-line arguments.
#

# GFLOW --conda-env=your-env-name
# GFLOW --gpus=1
# GFLOW --priority=10
# GFLOW --depends-on=123
# GFLOW --array=1-10
# GFLOW --time=1:30:00
# --- Your script starts here ---
echo "Starting gflow job..."
echo "Running on node: $HOSTNAME"
sleep 20
echo "Job finished successfully."
