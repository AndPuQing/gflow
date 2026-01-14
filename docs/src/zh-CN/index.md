---
layout: home

hero:
  name: "gflow"
  text: "轻量级任务调度器"
  tagline: "受 Slurm 启发的单节点任务调度器，专为高效管理配备 GPU 资源的机器上的任务而设计"
  actions:
    - theme: brand
      text: 快速开始
      link: /zh-CN/getting-started/installation
    - theme: alt
      text: 在 GitHub 上查看
      link: https://github.com/AndPuQing/gflow
  image:
    src: /logo.svg
    alt: gflow

features:
  - icon: 🚀
    title: 守护进程调度
    details: 持久化守护进程（gflowd）在后台运行，管理任务队列并自动分配资源。
  - icon: 📋
    title: 丰富的任务提交选项
    details: 提交任务时可指定 GPU 资源请求、依赖关系、优先级、时间限制、Conda 环境激活和任务数组。
  - icon: ⏱️
    title: 时间限制
    details: 为任务设置最大运行时间以防止失控进程，类似 Slurm 的 --time 选项。
  - icon: 🔗
    title: 任务依赖
    details: 创建复杂的工作流，其中任务依赖于其他任务，实现复杂的任务编排。
  - icon: 📊
    title: 强大的监控功能
    details: 使用灵活的选项查询和过滤任务，实时跟踪您的工作负载。
  - icon: 🖥️
    title: tmux 集成
    details: 每个任务都在自己的 tmux 会话中运行，允许您附加、实时查看输出并恢复中断的会话。
---
