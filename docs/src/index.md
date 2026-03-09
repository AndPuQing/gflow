---
layout: home

hero:
  name: "gflow"
  text: "Single-Node Job Scheduler"
  tagline: "Run, queue, inspect, and control GPU or CPU jobs on one Linux machine without deploying a cluster"
  actions:
    - theme: brand
      text: Quick Start
      link: /getting-started/quick-start
    - theme: alt
      text: Installation
      link: /getting-started/installation
    - theme: alt
      text: Command Reference
      link: /reference/quick-reference
  image:
    src: /logo.svg
    alt: gflow

features:
  - icon: 🚀
    title: Lightweight by Design
    details: Bring queueing and scheduling to a single machine without deploying a full cluster stack.
  - icon: 📋
    title: Flexible Submission
    details: Submit scripts or commands with GPU requests, priorities, time limits, Conda environments, and arrays.
  - icon: 🔗
    title: Dependencies and Arrays
    details: Chain jobs together, fan out workloads, and build small research pipelines with familiar CLI commands.
  - icon: 📊
    title: Queue Visibility
    details: Inspect active or completed jobs, filter by user or project, and switch between table, tree, JSON, CSV, or YAML output.
  - icon: 🖥️
    title: tmux-backed Execution
    details: Every job runs in its own tmux session so you can attach, follow logs, and recover long-running work.
  - icon: 🎛️
    title: GPU-aware Control
    details: Restrict visible GPUs, manage shared scheduling with VRAM limits, and apply runtime controls without editing scripts.
---
