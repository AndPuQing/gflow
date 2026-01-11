---
layout: home

hero:
  name: "gflow"
  text: "Lightweight Job Scheduler"
  tagline: "A single-node job scheduler inspired by Slurm, designed for efficiently managing tasks on machines with GPU resources"
  actions:
    - theme: brand
      text: Get Started
      link: /getting-started/installation
    - theme: alt
      text: View on GitHub
      link: https://github.com/AndPuQing/gflow
  image:
    src: /logo.svg
    alt: gflow

features:
  - icon: 🚀
    title: Daemon-based Scheduling
    details: A persistent daemon (gflowd) runs in the background, managing the job queue and automatically allocating resources.
  - icon: 📋
    title: Rich Job Submission
    details: Submit jobs with GPU resource requests, dependencies, priority levels, time limits, Conda environment activation, and job arrays.
  - icon: ⏱️
    title: Time Limits
    details: Set maximum runtime for jobs to prevent runaway processes, similar to Slurm's --time option.
  - icon: 🔗
    title: Job Dependencies
    details: Create complex workflows where jobs depend on others, enabling sophisticated task orchestration.
  - icon: 📊
    title: Powerful Monitoring
    details: Query and filter jobs with flexible options to track your workload in real-time.
  - icon: 🖥️
    title: tmux Integration
    details: Every job runs in its own tmux session, allowing you to attach, view output in real-time, and resume interrupted sessions.
---
